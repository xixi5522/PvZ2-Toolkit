// ============================================================================
// platform.rs —— Android 平台层（JNI 反射）
//
// 背景：本应用是 android:hasCode="false" 的纯 NativeActivity，没有 Java 层。
// 所有系统能力（权限、Toast、SharedPreferences、剪贴板、深色模式）
// 都得从 Rust 侧通过 JNI 反射去调 Android 框架的 Java API。
//
// 为什么不用 ndk crate 的现成封装：
//   NDK 只暴露 native 侧 API（ANativeActivity 之类），
//   而权限、SharedPreferences 这些只有 Java API，所以必须反射。
//
// ---- jni 0.21 API 的三个要点（踩过坑，记下来）----
//
//   1. `env.call_method(obj, name, sig, args)` 需要「方法名 + 签名」两个字符串，
//      不接受 JMethodID。所以不要先 get_method_id 再把它传给 call_method。
//
//   2. `find_class` 里的类名要用 JNI 的斜杠形式（android/content/Context），
//      内部类用 `$`（android/content/SharedPreferences$Editor）。
//
//   3. 从函数里返回的 jobject 必须转成 GlobalRef 的裸指针，
//      否则局部引用在 Native 方法返回后会被 JVM 清掉，
//      后续使用就是 use-after-free。
//
// 这一层所有函数都是「尽力而为」：失败就返回 None / 默认值，
// 绝不 panic。系统 API 在不同 Android 版本上差异很大，
// 让设置页少显示一个字段，远好过整个应用崩掉。
// ============================================================================

use jni::objects::{GlobalRef, JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};
use std::sync::OnceLock;

static JAVA_VM: OnceLock<JavaVM> = OnceLock::new();
// 用 GlobalRef 而不是裸指针：GlobalRef 会通知 JVM 这个引用要跨线程长期持有，
// GC 不会回收，Drop 时也会正确释放。裸 jobject 指针在这里不安全。
static ACTIVITY: OnceLock<GlobalRef> = OnceLock::new();

/// 从 `AndroidApp` 里取出 JavaVM 与 Activity，长期持有。
pub fn init(app: &slint::android::AndroidApp) {
    let vm_ptr = app.vm_as_ptr() as *mut jni::sys::JavaVM;
    let activity_ptr = app.activity_as_ptr();

    // SAFETY: vm_ptr 来自 android-activity，进程存活期间有效
    let vm = match unsafe { JavaVM::from_raw(vm_ptr) } {
        Ok(vm) => vm,
        Err(e) => {
            log::error!("platform: 取 JavaVM 失败 {e:?}");
            return;
        }
    };

    // 把 activity 的局部引用升级成全局引用。
    // 必须先 attach 到当前线程才能做 NewGlobalRef。
    if let Ok(env) = vm.attach_current_thread() {
        let local = unsafe { JObject::from_raw(activity_ptr as *mut jni::sys::_jobject) };
        if let Ok(g) = env.new_global_ref(&local) {
            let _ = ACTIVITY.set(g);
        }
    }

    let _ = JAVA_VM.set(vm);
    log::info!("platform: 已初始化 (activity={activity_ptr:p})");
}

/// 拿一个已 attach 到当前线程的 JNIEnv。
///
/// AttachGuard 在本线程首次 attach 时负责 Drop 时 detach，
/// 重复 attach 是空操作。所以可以安全地从任意线程反复调。
pub fn with_env<R>(f: impl FnOnce(&mut JNIEnv) -> R) -> Option<R> {
    let vm = JAVA_VM.get()?;
    let mut env = vm.attach_current_thread().ok()?;
    Some(f(&mut env))
}

/// Activity 对象（GlobalRef，进程存活期间有效）。
pub fn activity() -> Option<&'static GlobalRef> {
    ACTIVITY.get()
}

// ============================================================================
// 便利封装
//
// 下面几个小函数把「调一个 Java 方法」的样板收拢起来。
// 全部返回 Option，失败即 None —— 调用方不用处理几十种 JNI 错误类型。
// ============================================================================

/// 把一个可能被 JVM 回收的局部引用「提升」为永久存活的对象句柄。
///
/// 做法：new_global_ref 造一个全局引用，取出它底层的 jobject 指针，
/// 然后 forget 掉 GlobalRef 的析构 —— 这样 JVM 的全局引用表会一直持有它，
/// 但 Rust 侧的包装对象不再负责释放。对进程级单例来说这是有意的选择：
/// 这些对象活到进程结束，提前释放反而有风险。
fn promote(env: &mut JNIEnv, local: JObject) -> Option<JObject<'static>> {
    if local.is_null() {
        return None;
    }
    let g = env.new_global_ref(&local).ok()?;
    let raw = g.as_raw();
    std::mem::forget(g);
    // SAFETY: raw 指向 JVM 全局引用表里的一个有效 jobject，永不被回收
    Some(unsafe { JObject::from_raw(raw) })
}

/// 调一个无参、返回对象的方法。
fn call_obj(env: &mut JNIEnv, obj: &JObject, name: &str, sig: &str) -> Option<JObject<'static>> {
    let v = env.call_method(obj, name, sig, &[]).ok()?;
    promote(env, v.l().ok()?)
}

/// 调一个带参、返回对象的方法。
fn call_obj_args<'a>(
    env: &mut JNIEnv,
    obj: &JObject,
    name: &str,
    sig: &str,
    args: &[JValue<'a, 'a>],
) -> Option<JObject<'static>> {
    let v = env.call_method(obj, name, sig, args).ok()?;
    promote(env, v.l().ok()?)
}

/// 调一个返回 boolean 的无参方法。
fn call_bool(env: &mut JNIEnv, obj: &JObject, name: &str, sig: &str) -> Option<bool> {
    env.call_method(obj, name, sig, &[]).ok()?.z().ok()
}

/// JString → Rust String
///
/// jni 0.21 里 `JString::from` 只接受 owned JObject（没有 `From<&JObject>`），
/// 所以这里先 `unsafe { JString::from_raw(s.as_raw()) }` 借用同一个引用 ——
/// 不增加局部引用计数，用完即弃，不影响调用方的生命周期。
fn jstr(env: &mut JNIEnv, s: &JObject) -> String {
    if s.is_null() {
        return String::new();
    }
    // SAFETY: s 是一个有效的 java.lang.String 引用，from_raw 只借用不接管所有权
    let js = unsafe { JString::from_raw(s.as_raw()) };
    env.get_string(&js).map(|s| s.into()).unwrap_or_default()
}

/// 对象 → i64（试 Long）
fn call_long(env: &mut JNIEnv, obj: &JObject) -> Option<i64> {
    env.call_method(obj, "longValue", "()J", &[]).ok()?.j().ok()
}

/// 对象 → bool（试 Boolean）
fn call_bool_val(env: &mut JNIEnv, obj: &JObject) -> Option<bool> {
    env.call_method(obj, "booleanValue", "()Z", &[])
        .ok()?
        .z()
        .ok()
}

// ============================================================================
// 权限
// ============================================================================

/// 判断是否已授予读/写外部存储权限。
///
/// Context.checkSelfPermission(String) 返回 PackageManager.PERMISSION_GRANTED == 0
pub fn has_storage_permission() -> bool {
    let Some(activity) = activity() else {
        return false;
    };

    with_env(|env| {
        let act: &JObject = activity.as_obj();
        for perm in [
            "android.permission.READ_EXTERNAL_STORAGE",
            "android.permission.WRITE_EXTERNAL_STORAGE",
        ] {
            let Ok(p) = env.new_string(perm) else { continue };
            let pj = JObject::from(p);
            let ok = env
                .call_method(
                    act,
                    "checkSelfPermission",
                    "(Ljava/lang/String;)I",
                    &[JValue::Object(&pj)],
                )
                .ok()
                .and_then(|v| v.i().ok())
                .map(|code| code == 0)
                .unwrap_or(false);
            if ok {
                return true;
            }
        }
        false
    })
    .unwrap_or(false)
}

/// 发起存储权限申请。
///
/// 走 Activity.requestPermissions(String[], int)。结果通过
/// onRequestPermissionsResult 回调 —— 那个回调在 NativeActivity 里
/// 不会转发给 Rust，所以这里不去等结果，
/// 由调用方过一会儿再查一次 has_storage_permission()。
pub fn request_storage_permission() {
    let Some(activity) = activity() else { return };

    with_env(|env| {
        let act: &JObject = activity.as_obj();
        let r = (|| -> Result<(), jni::errors::Error> {
            let perms = [
                "android.permission.READ_EXTERNAL_STORAGE",
                "android.permission.WRITE_EXTERNAL_STORAGE",
            ];
            let jarr =
                env.new_object_array(perms.len() as i32, "java/lang/String", JObject::null())?;
            for (i, p) in perms.iter().enumerate() {
                let s = env.new_string(p)?;
                env.set_object_array_element(&jarr, i as i32, &s)?;
            }
            let jarr = JObject::from(jarr);

            // 1001 是自定义请求码，在自己的代码里唯一即可
            env.call_method(
                act,
                "requestPermissions",
                "([Ljava/lang/String;I)V",
                &[JValue::Object(&jarr), JValue::Int(1001)],
            )?;
            Ok(())
        })();

        if let Err(e) = r {
            log::warn!("platform: 申请存储权限失败 {e:?}");
        }
    });
}

// ============================================================================
// Toast
// ============================================================================

/// 弹一个系统 Toast。失败时只记日志。
pub fn toast(msg: &str) {
    let Some(activity) = activity() else { return };

    with_env(|env| {
        let act: &JObject = activity.as_obj();
        let r = (|| -> Result<(), jni::errors::Error> {
            let text = env.new_string(msg)?;
            let jt = JObject::from(text);

            // Toast.makeText 是静态方法，前两个参数是 Context 和 CharSequence
            let toast = env.call_static_method(
                "android/widget/Toast",
                "makeText",
                "(Landroid/content/Context;Ljava/lang/CharSequence;I)Landroid/widget/Toast;",
                &[
                    JValue::Object(act),
                    JValue::Object(&jt),
                    JValue::Int(0), // Toast.LENGTH_SHORT
                ],
            )?;
            let t = toast.l()?;
            env.call_method(&t, "show", "()V", &[])?;
            Ok(())
        })();

        if let Err(e) = r {
            log::warn!("platform: Toast 失败 {e:?}");
        }
    });
}

// ============================================================================
// SharedPreferences
// ============================================================================

/// 读取全部键值，拼成 "key=value\n" 文本，交给 settings.rs 解析。
///
/// 为什么拼成文本而不是返回 HashMap：
/// settings.rs 里已有一套解析逻辑（桌面端读文件用同一套），
/// 复用它能让两端行为完全一致。
pub fn prefs_get_all(name: &str) -> Option<String> {
    let activity = activity()?;

    with_env(|env| {
        let act: &JObject = activity.as_obj();

        // 这一串调用任何一步失败就整体返回 None
        let jname = env.new_string(name).ok()?;
        let jname = JObject::from(jname);

        let prefs = call_obj_args(
            env,
            act,
            "getSharedPreferences",
            "(Ljava/lang/String;I)Landroid/content/SharedPreferences;",
            &[JValue::Object(&jname), JValue::Int(0)], // MODE_PRIVATE
        )?;

        let map = call_obj(env, &prefs, "getAll", "()Ljava/util/Map;")?;
        let key_set = call_obj(env, &map, "keySet", "()Ljava/util/Set;")?;
        let iter = call_obj(env, &key_set, "iterator", "()Ljava/util/Iterator;")?;

        let mut out = String::new();
        // 防御性上限：万一 iterator() 实现有问题也不会死循环
        for _ in 0..512 {
            let more = call_bool(env, &iter, "hasNext", "()Z").unwrap_or(false);
            if !more {
                break;
            }
            let Some(k) = call_obj(env, &iter, "next", "()Ljava/lang/Object;") else {
                break;
            };
            let Some(v) = call_obj_args(
                env,
                &map,
                "get",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                &[JValue::Object(&k)],
            ) else {
                continue;
            };

            let ks = jstr(env, &k);
            if ks.is_empty() {
                continue;
            }

            // 值可能是 String / Long / Boolean —— SharedPreferences 的
            // getAll() 会把 int 当 Long 返回，所以先试 String 再试 Long 再试 Boolean
            let vjs = unsafe { JString::from_raw(v.as_raw()) };
            let vstr = if let Ok(s) = env.get_string(&vjs) {
                s.into()
            } else if let Some(n) = call_long(env, &v) {
                n.to_string()
            } else if let Some(b) = call_bool_val(env, &v) {
                b.to_string()
            } else {
                String::new()
            };

            out.push_str(&ks);
            out.push('=');
            out.push_str(&vstr);
            out.push('\n');
        }
        Some(out)
    })
    .flatten()
}

/// 批量写入键值。
///
/// 数值型的（threads）用 putInt 写，其余 putString ——
/// 读回来时更省事，不用猜类型。
pub fn prefs_put_all(name: &str, mode: i32, pairs: &[(&str, String)]) -> Result<(), String> {
    let activity = activity().ok_or("Activity 不可用")?;

    with_env(|env| {
        let act: &JObject = activity.as_obj();
        let r = (|| -> Result<(), jni::errors::Error> {
            let jname = env.new_string(name)?;
            let jname = JObject::from(jname);

            let prefs = env
                .call_method(
                    act,
                    "getSharedPreferences",
                    "(Ljava/lang/String;I)Landroid/content/SharedPreferences;",
                    &[JValue::Object(&jname), JValue::Int(mode)],
                )?
                .l()?;

            let editor = env
                .call_method(
                    &prefs,
                    "edit",
                    "()Landroid/content/SharedPreferences$Editor;",
                    &[],
                )?
                .l()?;

            for (k, v) in pairs {
                let jk = env.new_string(k)?;
                let jk = JObject::from(jk);

                if *k == "threads" {
                    let n: i32 = v.trim().parse().unwrap_or(0);
                    env.call_method(
                        &editor,
                        "putInt",
                        "(Ljava/lang/String;I)Landroid/content/SharedPreferences$Editor;",
                        &[JValue::Object(&jk), JValue::Int(n)],
                    )?;
                } else {
                    let jv = env.new_string(v)?;
                    let jv = JObject::from(jv);
                    env.call_method(
                        &editor,
                        "putString",
                        "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;",
                        &[JValue::Object(&jk), JValue::Object(&jv)],
                    )?;
                }
            }

            env.call_method(&editor, "apply", "()V", &[])?;
            Ok(())
        })();

        r.map_err(|e| format!("写入 SharedPreferences 失败：{e:?}"))
    })
    .unwrap_or_else(|| Err("JNI 环境不可用".into()))
}

// ============================================================================
// 剪贴板
// ============================================================================

/// 往系统剪贴板写文本。
pub fn set_clipboard(text: &str) -> Result<(), String> {
    let activity = activity().ok_or("Activity 不可用")?;

    with_env(|env| {
        let act: &JObject = activity.as_obj();
        let r = (|| -> Result<(), jni::errors::Error> {
            let svc_name = env.new_string("clipboard")?;
            let svc_name = JObject::from(svc_name);

            let cm = env
                .call_method(
                    act,
                    "getSystemService",
                    "(Ljava/lang/String;)Ljava/lang/Object;",
                    &[JValue::Object(&svc_name)],
                )?
                .l()?;

            let jtext = env.new_string(text)?;
            let jtext = JObject::from(jtext);

            env.call_method(
                &cm,
                "setText",
                "(Ljava/lang/CharSequence;)V",
                &[JValue::Object(&jtext)],
            )?;
            Ok(())
        })();

        r.map_err(|e| format!("写剪贴板失败：{e:?}"))
    })
    .unwrap_or_else(|| Err("JNI 环境不可用".into()))
}

/// 从系统剪贴板读文本。没有内容或不是文本时返回 `Ok(None)`。
///
/// 三跳调用链：
/// `ClipboardManager.getPrimaryClip() -> ClipData`
/// `ClipData.getItemAt(0)` -> `ClipData$Item`
/// `ClipData$Item.getText()` -> `CharSequence` -> `toString()` -> `String`
///
/// 两个坑：
/// 1. 内部类在 JNI 签名里必须写 `$` 而不是 `.`。写成
///    `Landroid/content/ClipData.Item;` 会抛 NoSuchMethodError ——
///    而且是在「取 getText 的返回值」这一步才炸，报错信息很难联想到签名问题。
/// 2. `getPrimaryClip()` 可能返回 null（剪贴板空 / 用户在别的应用清了），
///    所以必须判空再往下走，不能直接 call_method。
pub fn get_clipboard() -> Result<Option<String>, String> {
    let activity = activity().ok_or("Activity 不可用")?;

    with_env(|env| {
        let act: &JObject = activity.as_obj();
        let r = (|| -> Result<Option<String>, jni::errors::Error> {
            let svc_name = env.new_string("clipboard")?;
            let svc_name = JObject::from(svc_name);

            let cm = env
                .call_method(
                    act,
                    "getSystemService",
                    "(Ljava/lang/String;)Ljava/lang/Object;",
                    &[JValue::Object(&svc_name)],
                )?
                .l()?;

            // --- ClipData clip = cm.getPrimaryClip(); ---
            let clip = env
                .call_method(&cm, "getPrimaryClip", "()Landroid/content/ClipData;", &[])?
                .l()?;
            if clip.is_null() {
                return Ok(None);
            }

            // --- ClipData.Item item = clip.getItemAt(0); ---
            //
            // 注意签名里的 $ ：ClipData$Item 是**内部类**的写法。
            // Java 源码里写 ClipData.Item，但 JNI 描述符必须用 $。
            // 写成 "." 会抛 NoSuchMethodError。
            let item = env
                .call_method(
                    &clip,
                    "getItemAt",
                    "(I)Landroid/content/ClipData$Item;",
                    &[JValue::Int(0)],
                )?
                .l()?;
            if item.is_null() {
                return Ok(None);
            }

            // --- CharSequence text = item.getText(); ---
            let text = env
                .call_method(&item, "getText", "()Ljava/lang/CharSequence;", &[])?
                .l()?;
            if text.is_null() {
                // 是图片/URI 之类的非文本内容
                return Ok(None);
            }

            // CharSequence 是接口，真正能调的是它上面声明的 Object 方法。
            // toString() 是 java.lang.Object 的，用 CharSequence 的签名也行
            // —— 这里用 Object 签名，Dalvik 会顺着继承链找到实现。
            let s = env
                .call_method(&text, "toString", "()Ljava/lang/String;", &[])?
                .l()?;
            if s.is_null() {
                return Ok(None);
            }

            let jstr = unsafe { JString::from_raw(s.as_raw()) };
            let rust_str: String = env.get_string(&jstr)?.into();
            Ok(Some(rust_str))
        })();

        r.map_err(|e| format!("读剪贴板失败：{e:?}"))
    })
    .unwrap_or_else(|| Err("JNI 环境不可用".into()))
}

// ============================================================================
// 系统深色模式
// ============================================================================

/// 读系统的深色模式开关。
///
/// Resources.getConfiguration().uiMode & UI_MODE_NIGHT_MASK == UI_MODE_NIGHT_YES
/// 常量：UI_MODE_NIGHT_MASK = 0x30, UI_MODE_NIGHT_YES = 0x20
pub fn system_dark_mode() -> Option<bool> {
    let activity = activity()?;

    with_env(|env| {
        let act: &JObject = activity.as_obj();
        let res = call_obj(env, act, "getResources", "()Landroid/content/res/Resources;")?;
        let cfg = call_obj(
            env,
            &res,
            "getConfiguration",
            "()Landroid/content/res/Configuration;",
        )?;

        // uiMode 是 public 字段。jni 0.21 的 get_field 同样要「字段名 + 签名」，
        // 不需要先 get_field_id。
        let ui_mode = env.get_field(&cfg, "uiMode", "I").ok()?.i().ok()?;

        Some((ui_mode & 0x30) == 0x20)
    })
    .flatten()
}

// ============================================================================
// 设备信息
// ============================================================================

/// 主 ABI。编译期就能确定，不需要反射。
pub fn primary_abi() -> String {
    match std::env::consts::ARCH {
        "aarch64" => "arm64-v8a".to_string(),
        "arm" => "armeabi-v7a".to_string(),
        "x86_64" => "x86_64".to_string(),
        "x86" => "x86".to_string(),
        other => other.to_string(),
    }
}

/// 应用的私有文件目录（Context.getFilesDir()）。
///
/// 用途：处理大文件时的临时落盘位置。这个目录不需要任何权限，永远可写。
pub fn files_dir() -> String {
    let Some(activity) = activity() else {
        return String::new();
    };

    with_env(|env| {
        let act: &JObject = activity.as_obj();
        let dir = call_obj(env, act, "getFilesDir", "()Ljava/io/File;")?;
        let p = call_obj(env, &dir, "getAbsolutePath", "()Ljava/lang/String;")?;
        Some(jstr(env, &p))
    })
    .flatten()
    .unwrap_or_default()
}

/// 读系统 Material You 强调色（「跟随系统莫奈」的种子色来源）。
///
/// 读取链：WallpaperManager.getWallpaperColors(FLAG_SYSTEM)
///         → WallpaperColors.getPrimaryColor() → Color.toArgb()
///
/// 这是 Android 12+ 动态取色用的同一份数据（壁纸主色），API 24+ 可用，
/// 不需要额外权限。失败（取不到壁纸 / 厂商 ROM 裁剪 / JNI 异常）返回
/// None，调用方回退默认种子色。
pub fn system_accent_color() -> Option<u32> {
    let activity = activity()?;

    with_env(|env| {
        let act: &JObject = activity.as_obj();

        // WallpaperManager.getInstance(context)（静态方法比
        // getSystemService(WALLPAPER_SERVICE) 更直白，且两者等价）
        let wm_class = env.find_class("android/app/WallpaperManager").ok()?;
        let wm = env
            .call_static_method(
                &wm_class,
                "getInstance",
                "(Landroid/content/Context;)Landroid/app/WallpaperManager;",
                &[JValue::Object(act)],
            )
            .ok()?
            .l()
            .ok()?;

        // getWallpaperColors(FLAG_SYSTEM=1) —— 可能为 null（无壁纸）
        let colors = env
            .call_method(&wm, "getWallpaperColors", "(I)Landroid/app/WallpaperColors;", &[1i32.into()])
            .ok()?
            .l()
            .ok()?;
        if colors.is_null() {
            return None;
        }

        // getPrimaryColor() → android.graphics.Color
        let color = env
            .call_method(&colors, "getPrimaryColor", "()Landroid/graphics/Color;", &[])
            .ok()?
            .l()
            .ok()?;

        // toArgb() → int
        let argb = env
            .call_method(&color, "toArgb", "()I", &[])
            .ok()?
            .i()
            .ok()?;

        Some(argb as u32)
    })
    .flatten()
}

/// 外部存储根目录（getExternalFilesDir(null) 的父目录）。
///
/// 给用户一个「上次在这里」的默认起点 —— 用户放 .rsb 文件的地方
/// 通常是外部存储根目录下的某个文件夹。
pub fn external_storage_dir() -> Option<String> {
    let activity = activity()?;

    with_env(|env| {
        let act: &JObject = activity.as_obj();

        let dir = call_obj_args(
            env,
            act,
            "getExternalFilesDir",
            "(Ljava/lang/String;)Ljava/io/File;",
            &[JValue::Object(&JObject::null())],
        )?;
        if dir.is_null() {
            return None;
        }

        let parent = call_obj(env, &dir, "getParentFile", "()Ljava/io/File;")?;
        if parent.is_null() {
            return None;
        }

        let p = call_obj(env, &parent, "getAbsolutePath", "()Ljava/lang/String;")?;
        Some(jstr(env, &p))
    })
    .flatten()
}
