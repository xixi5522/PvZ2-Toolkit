// ============================================================================
// catalog.rs —— 功能目录（数据驱动）
//
// 本版把旧的自研 core 全部移除，处理层换成 LambdaEd1th/ed1ths-pvz-toolkit
// 的 formats 层（14 个格式库，AGPL-3.0-or-later）。
// 这张表就是那 14 个格式库在移动端的操作矩阵：
//
//   * 资源包类：RSB / RSGP / RSBP 补丁 / PAK / DZip
//   * 数据类：  RTON / SMF / 编译文本 / Crypt-Data / Newton 清单
//   * 动画类：  PAM / ReAnim / 粒子特效
//   * 音频类：  BNK / WEM
//
// 加一个功能 = 在下面加一条记录，UI 不用动（搜索/表单/执行全都是通用的）。
// ============================================================================

/// 参数种类。决定了 UI 上渲染成什么控件，以及取值时怎么解析。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    /// 输入文件：渲染成「选择文件」按钮 + 只读路径显示
    InputFile,
    /// 输入目录：渲染成「选择目录」按钮 + 只读路径显示
    InputDir,
    /// 输出文件：可留空（留空时用默认命名规则）
    OutputFile,
    /// 输出目录：可留空
    OutputDir,
    /// 布尔开关
    Bool,
    /// 整数输入
    Int,
    /// 文本输入
    Text,
    /// 枚举下拉框，取值来自 Param::choices
    Choice,
}

/// 一个参数的完整描述。
#[derive(Debug, Clone)]
pub struct Param {
    /// 内部键名，用于从 UI 回传的表单里取值。必须全局唯一（同一命令内）。
    pub key: &'static str,
    /// 显示名（中文）
    pub label: &'static str,
    /// 控件类型
    pub kind: ParamKind,
    /// 是否必填。OutputFile/OutputDir 一般为 false（留空走默认）。
    pub required: bool,
    /// 默认值（文本形式；Int 会 parse，Bool 用 "true"/"false"）
    pub default: &'static str,
    /// 说明文字，显示在控件下方做提示
    pub hint: &'static str,
    /// Choice 类型的候选项；其它类型留空
    pub choices: &'static [&'static str],
}

impl Param {
    /// 构造一个必填参数的最简写法。
    const fn new(
        key: &'static str,
        label: &'static str,
        kind: ParamKind,
        hint: &'static str,
    ) -> Self {
        Self {
            key,
            label,
            kind,
            required: true,
            default: "",
            hint,
            choices: &[],
        }
    }

    /// 标记为可选项（输出路径之类）。
    const fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    /// 给默认值。
    const fn with_default(mut self, v: &'static str) -> Self {
        self.default = v;
        self
    }

    /// 给枚举候选项。
    const fn with_choices(mut self, c: &'static [&'static str]) -> Self {
        self.choices = c;
        self
    }
}

// ---- 参数构造的语法糖，让下面的表读起来短一些 ------------------------------

const fn file_in(key: &'static str, label: &'static str, hint: &'static str) -> Param {
    Param::new(key, label, ParamKind::InputFile, hint)
}
const fn dir_in(key: &'static str, label: &'static str, hint: &'static str) -> Param {
    Param::new(key, label, ParamKind::InputDir, hint)
}
const fn file_out(key: &'static str, label: &'static str, hint: &'static str) -> Param {
    Param::new(key, label, ParamKind::OutputFile, hint).optional()
}
const fn dir_out(key: &'static str, label: &'static str, hint: &'static str) -> Param {
    Param::new(key, label, ParamKind::OutputDir, hint).optional()
}
const fn flag(key: &'static str, label: &'static str, hint: &'static str) -> Param {
    Param::new(key, label, ParamKind::Bool, hint)
        .optional()
        .with_default("false")
}
const fn choice(
    key: &'static str,
    label: &'static str,
    hint: &'static str,
    choices: &'static [&'static str],
    default: &'static str,
) -> Param {
    Param::new(key, label, ParamKind::Choice, hint)
        .with_choices(choices)
        .with_default(default)
}
const fn text(key: &'static str, label: &'static str, hint: &'static str) -> Param {
    Param::new(key, label, ParamKind::Text, hint)
}

/// 一个功能（命令）的完整描述。
#[derive(Debug, Clone)]
pub struct Command {
    /// 稳定 ID（分发用，如 "rsb.unpack"）
    pub id: &'static str,
    /// 显示名（中文）
    pub name: &'static str,
    /// 分类（中文），也是搜索与分组维度
    pub category: &'static str,
    /// 一句话说明
    pub desc: &'static str,
    /// 参数表
    pub params: &'static [Param],
}

impl Command {
    /// 搜索匹配：ID / 名称 / 分类 / 说明 任一命中即算。
    pub fn matches(&self, q: &str) -> bool {
        let q = q.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        self.id.to_lowercase().contains(&q)
            || self.name.contains(q.as_str())
            || self.category.contains(q.as_str())
            || self.desc.contains(q.as_str())
    }
}

// ============================================================================
// 功能表 —— 14 个格式库 × 各自的移动端操作
// ============================================================================

pub static COMMANDS: &[Command] = &[
    // ---------------------------------------------------------------- RSB
    Command {
        id: "rsb.unpack",
        name: "RSB 解包",
        category: "RSB 资源包",
        desc: "把 .rsb/.rsc 资源包解开成文件，可选自动把 PTX 贴图转成 PNG",
        params: &[
            file_in("input", "RSB 文件", "选择 main.rsb 之类的资源包"),
            dir_out("output", "输出目录", "留空则输出到资源包旁边的目录"),
            flag("ptx-png", "PTX 转 PNG", "解包时把贴图解码为 PNG（软件解码，支持 ETC1/PVRTC/ASTC）"),
        ],
    },
    Command {
        id: "rsb.ptx-export",
        name: "RSB 贴图导出",
        category: "RSB 资源包",
        desc: "只导出包内全部 PTX 贴图为 PNG，不解其它文件",
        params: &[
            file_in("input", "RSB 文件", "选择资源包"),
            dir_out("output", "输出目录", "PNG 输出位置"),
        ],
    },
    Command {
        id: "rsb.pack",
        name: "RSB 回包",
        category: "RSB 资源包",
        desc: "把修改后的解包目录重新打回 .rsb；改动过的贴图 PNG 自动按原格式编码回 PTX",
        params: &[
            file_in("input", "原版 RSB", "未改动的原资源包（提供包结构与贴图参数基准）"),
            dir_in("dir", "修改后的解包目录", "rsb.unpack 输出的、已修改内容的目录"),
            file_out("output", "输出 RSB", "留空自动命名"),
        ],
    },
    Command {
        id: "ptx.decode",
        name: "PTX 转 PNG",
        category: "RSB 资源包",
        desc: "把独立 .ptx 贴图解码为 PNG（PTX 无文件头，需提供宽高与格式码）",
        params: &[
            file_in("input", "PTX 文件", "选择独立 .ptx 文件"),
            Param::new("width", "宽度", ParamKind::Int, "贴图像素宽（可在解包输出的 textures.json 里查）"),
            Param::new("height", "高度", ParamKind::Int, "贴图像素高"),
            Param::new(
                "format",
                "格式码",
                ParamKind::Text,
                "数字码或名称：0=Rgba8888 2=Rgb565 30=Pvrtc4 147=Etc1 160=Astc4x4；30/147 加 alpha_size 为调色板 ETC",
            ),
            Param::new("alpha-size", "Alpha 大小", ParamKind::Int, "调色板/附加字节数，未知填 0")
                .optional()
                .with_default("0"),
            Param::new("alpha-format", "Alpha 格式", ParamKind::Int, "兼容字段 scale，未知填 0")
                .optional()
                .with_default("0"),
            Param::new("pitch", "行距", ParamKind::Int, "每行字节数，未知填 0")
                .optional()
                .with_default("0"),
            choice("order", "通道序", "Rgba：Android/PC；Bgra：iOS 纹理", &["Rgba", "Bgra"], "Rgba"),
            file_out("output", "输出 PNG", "留空自动命名"),
        ],
    },
    Command {
        id: "ptx.encode",
        name: "PNG 转 PTX",
        category: "RSB 资源包",
        desc: "把 PNG 编码为独立 .ptx 贴图（供 ptx.decode 校验或手工放进包目录）",
        params: &[
            file_in("input", "PNG 文件", "选择要编码的 PNG（块压缩格式要求边长为块大小倍数）"),
            choice(
                "format",
                "目标格式",
                "回包用：与原贴图格式一致即可在 textures.json 里查到",
                &[
                    "Rgba8888", "Rgba4444", "Rgb565", "Rgba5551",
                    "Etc1", "Etc1A8", "Etc1CompressedAlpha", "Etc1Palette",
                    "Pvrtc4BppRgba", "Pvrtc4BppRgbaA8",
                    "Astc4x4", "Astc5x5", "Astc6x6", "Astc8x8",
                ],
                "Rgba8888",
            ),
            choice("order", "通道序", "Rgba：Android/PC；Bgra：iOS 纹理", &["Rgba", "Bgra"], "Rgba"),
            Param::new("pitch", "行距", ParamKind::Int, "每行字节数，留 0 自动计算")
                .optional()
                .with_default("0"),
            file_out("output", "输出 PTX", "留空自动命名"),
        ],
    },
    // ---------------------------------------------------------------- RSGP
    Command {
        id: "rsgp.unpack",
        name: "RSGP 分包解包",
        category: "RSB 资源包",
        desc: "把 .rsgp 分包文件解开成内部文件（附 manifest.json 记录贴图信息）",
        params: &[
            file_in("input", "RSGP 文件", "选择 .rsgp 文件"),
            dir_out("output", "输出目录", "留空则输出到分包旁边的目录"),
        ],
    },
    Command {
        id: "rsgp.pack",
        name: "RSGP 分包打包",
        category: "RSB 资源包",
        desc: "把目录重新打包为 .rsgp 分包（rsgp.unpack 输出目录可直接回包）",
        params: &[
            dir_in("input", "输入目录", "rsgp.unpack 输出的、已修改内容的目录（含 manifest.json）"),
            choice("version", "RSG 版本", "4：现代版本；3：旧版本", &["4", "3"], "4"),
            choice(
                "flags",
                "压缩方式",
                "3：part0+part1 全压缩；2：仅 part0；1：仅 part1；0：不压缩",
                &["3", "2", "1", "0"],
                "3",
            ),
            file_out("output", "输出 RSGP", "留空自动命名"),
        ],
    },
    // ---------------------------------------------------------------- RSBP
    Command {
        id: "rsbp.create",
        name: "RSB 补丁生成",
        category: "RSB 补丁",
        desc: "对比旧版与新版 RSB，生成体积只含差异的补丁文件（.rsbp）",
        params: &[
            file_in("before", "旧版 RSB", "原版资源包"),
            file_in("after", "新版 RSB", "修改后的资源包"),
            file_out("output", "输出补丁", "留空自动命名"),
            choice(
                "mode",
                "打包模式",
                "Stored：与官方 twinning 兼容的存储模式；Raw：原始包模式",
                &["stored", "raw"],
                "stored",
            ),
        ],
    },
    Command {
        id: "rsbp.apply",
        name: "RSB 补丁应用",
        category: "RSB 补丁",
        desc: "把补丁应用到基础 RSB 上，合成新版资源包",
        params: &[
            file_in("base", "基础 RSB", "未修改的原版资源包"),
            file_in("patch", "补丁文件", "选择 .rsbp 补丁"),
            file_out("output", "输出 RSB", "留空自动命名"),
            choice(
                "mode",
                "打包模式",
                "需要与生成补丁时的模式一致",
                &["stored", "raw"],
                "stored",
            ),
        ],
    },
    // ---------------------------------------------------------------- PAK
    Command {
        id: "pak.unpack",
        name: "PAK 解包",
        category: "PAK 资源包",
        desc: "解开 .pak 资源包（PopCap zip 变体）为目录",
        params: &[
            file_in("input", "PAK 文件", "选择 .pak 文件"),
            dir_out("output", "输出目录", "留空则输出到包旁边的目录"),
        ],
    },
    Command {
        id: "pak.pack",
        name: "PAK 打包",
        category: "PAK 资源包",
        desc: "把目录打包回 .pak 资源包",
        params: &[
            dir_in("input", "输入目录", "要打包的目录"),
            file_out("output", "输出 PAK", "留空自动命名"),
        ],
    },
    Command {
        id: "pak.list",
        name: "PAK 清单导出",
        category: "PAK 资源包",
        desc: "列出包内全部条目（路径/大小）导出为文本清单",
        params: &[
            file_in("input", "PAK 文件", "选择 .pak 文件"),
            file_out("output", "输出清单", "留空自动命名"),
        ],
    },
    // ---------------------------------------------------------------- DZip
    Command {
        id: "dzip.unpack",
        name: "DZip 解包",
        category: "DZip 资源包",
        desc: "解开 main.dzip 及其 GAME.00x 卷文件为目录",
        params: &[
            file_in("input", "DZip 文件", "选择 main.dzip 之类的索引文件"),
            dir_out("output", "输出目录", "留空则输出到索引文件旁边的目录"),
            dir_in(
                "volumes",
                "卷文件目录",
                "GAME.00x 所在目录；留空则用 DZip 文件所在目录",
            )
            .optional(),
        ],
    },
    Command {
        id: "dzip.pack",
        name: "DZip 打包",
        category: "DZip 资源包",
        desc: "把目录打包为 .dzip 资源包",
        params: &[
            dir_in("input", "输入目录", "要打包的目录"),
            file_out("output", "输出 DZip", "留空自动命名"),
        ],
    },
    // ---------------------------------------------------------------- RTON
    Command {
        id: "rton.decode",
        name: "RTON 解码",
        category: "RTON 数据",
        desc: "二进制 RTON → 可读 JSON（支持加密 RTON）",
        params: &[
            file_in("input", "RTON 文件", "选择 .rton 文件"),
            file_out("output", "输出 JSON", "留空自动命名"),
            flag("encrypted", "加密 RTON", "文件被 Rijndael 整体加密时打开"),
        ],
    },
    Command {
        id: "rton.encode",
        name: "RTON 编码",
        category: "RTON 数据",
        desc: "JSON → 二进制 RTON",
        params: &[
            file_in("input", "JSON 文件", "选择解码出的 JSON"),
            file_out("output", "输出 RTON", "留空自动命名"),
            flag("encrypt", "输出加密", "生成整体加密的 RTON"),
        ],
    },
    // ---------------------------------------------------------------- SMF
    Command {
        id: "smf.decode",
        name: "SMF 解码",
        category: "SMF 容器",
        desc: "解开 .smf 容器得到内部文本/数据",
        params: &[
            file_in("input", "SMF 文件", "选择 .smf 文件"),
            file_out("output", "输出文件", "留空自动命名"),
        ],
    },
    Command {
        id: "smf.encode",
        name: "SMF 编码",
        category: "SMF 容器",
        desc: "把文件重新封装为 .smf 容器",
        params: &[
            file_in("input", "输入文件", "解码出的原始内容"),
            file_out("output", "输出 SMF", "留空自动命名"),
        ],
    },
    // ---------------------------------------------------------------- 编译文本
    Command {
        id: "ctext.decode",
        name: "编译文本解码",
        category: "编译文本",
        desc: "解密编译文本（如 LAWNSTRINGS.TXT）为明文",
        params: &[
            file_in("input", "编译文本", "选择加密文本文件"),
            text("seed", "SEED 密钥", "加密种子字符串"),
            file_out("output", "输出文件", "留空自动命名"),
        ],
    },
    Command {
        id: "ctext.encode",
        name: "编译文本编码",
        category: "编译文本",
        desc: "把明文重新加密为编译文本",
        params: &[
            file_in("input", "明文文件", "要加密的文本"),
            text("seed", "SEED 密钥", "加密种子字符串"),
            file_out("output", "输出文件", "留空自动命名"),
        ],
    },
    // ---------------------------------------------------------------- Crypt-Data
    Command {
        id: "cdat.decode",
        name: "Crypt-Data 解密",
        category: "Crypt-Data",
        desc: "解密 .cdat 之类的加密数据块",
        params: &[
            file_in("input", "加密数据", "选择加密文件"),
            text("key", "密钥", "加密密钥（按下方格式解析）"),
            choice(
                "key-format",
                "密钥格式",
                "utf8：直接按文本使用；hex：按十六进制解析",
                &["utf8", "hex"],
                "utf8",
            ),
            file_out("output", "输出文件", "留空自动命名"),
        ],
    },
    Command {
        id: "cdat.encode",
        name: "Crypt-Data 加密",
        category: "Crypt-Data",
        desc: "用密钥加密数据块",
        params: &[
            file_in("input", "明文数据", "要加密的文件"),
            text("key", "密钥", "加密密钥（按下方格式解析）"),
            choice(
                "key-format",
                "密钥格式",
                "utf8：直接按文本使用；hex：按十六进制解析",
                &["utf8", "hex"],
                "utf8",
            ),
            file_out("output", "输出文件", "留空自动命名"),
        ],
    },
    // ---------------------------------------------------------------- Newton
    Command {
        id: "newton.decode",
        name: "Newton 解码",
        category: "Newton 清单",
        desc: "RESOURCES.NEWTON 资源清单 → JSON",
        params: &[
            file_in("input", "Newton 文件", "选择 .newton / RESOURCES.NEWTON"),
            file_out("output", "输出 JSON", "留空自动命名"),
        ],
    },
    Command {
        id: "newton.encode",
        name: "Newton 编码",
        category: "Newton 清单",
        desc: "JSON → RESOURCES.NEWTON",
        params: &[
            file_in("input", "JSON 文件", "选择清单 JSON"),
            file_out("output", "输出 Newton", "留空自动命名"),
        ],
    },
    // ---------------------------------------------------------------- PAM
    Command {
        id: "pam.decode",
        name: "PAM 解码",
        category: "PAM 动画",
        desc: "二进制 PAM 动画 → JSON（帧/轨道信息）",
        params: &[
            file_in("input", "PAM 文件", "选择 .pam 文件"),
            file_out("output", "输出 JSON", "留空自动命名"),
        ],
    },
    Command {
        id: "pam.encode",
        name: "PAM 编码",
        category: "PAM 动画",
        desc: "JSON → 二进制 PAM 动画",
        params: &[
            file_in("input", "JSON 文件", "选择 PAM JSON"),
            file_out("output", "输出 PAM", "留空自动命名"),
        ],
    },
    // ---------------------------------------------------------------- ReAnim
    Command {
        id: "reanim.decode",
        name: "ReAnim 解码",
        category: "ReAnim 动画",
        desc: "二进制 .reanim → JSON（自动识别 PC / 手机版本）",
        params: &[
            file_in("input", "ReAnim 文件", "选择 .reanim 文件"),
            file_out("output", "输出 JSON", "留空自动命名"),
        ],
    },
    Command {
        id: "reanim.encode",
        name: "ReAnim 编码",
        category: "ReAnim 动画",
        desc: "JSON → 二进制 .reanim",
        params: &[
            file_in("input", "JSON 文件", "选择 reanim JSON"),
            choice(
                "version",
                "目标版本",
                "PC：桌面版格式；Phone32/Phone64：手机版格式",
                &["pc", "phone32", "phone64"],
                "pc",
            ),
            file_out("output", "输出 ReAnim", "留空自动命名"),
        ],
    },
    Command {
        id: "reanim.xfl-decode",
        name: "XFL 工程导入",
        category: "ReAnim 动画",
        desc: "把 XFL 动画工程目录（DOMDocument.xml）转成 ReAnim JSON",
        params: &[
            dir_in("input", "XFL 工程目录", "含 DOMDocument.xml 的 Flash/Animate 工程目录"),
            file_out("output", "输出 JSON", "留空自动命名"),
        ],
    },
    Command {
        id: "reanim.xfl-encode",
        name: "ReAnim 转 XFL 工程",
        category: "ReAnim 动画",
        desc: "把 ReAnim JSON 还原成可在 Flash/Animate 打开的 XFL 工程目录",
        params: &[
            file_in("input", "ReAnim JSON", "reanim.decode 输出的 JSON"),
            dir_out("output", "输出目录", "XFL 工程输出位置（留空自动命名）"),
        ],
    },
    // ---------------------------------------------------------------- 粒子
    Command {
        id: "popfx.decode",
        name: "粒子特效解码",
        category: "粒子特效",
        desc: "二进制粒子库 .popfx → XML",
        params: &[
            file_in("input", "粒子文件", "选择 .popfx 粒子库"),
            choice(
                "version",
                "文件版本",
                "自动：依次尝试 PC 与手机版本；也可强制指定",
                &["auto", "pc", "phone32", "phone64"],
                "auto",
            ),
            file_out("output", "输出 XML", "留空自动命名"),
        ],
    },
    Command {
        id: "popfx.encode",
        name: "粒子特效编码",
        category: "粒子特效",
        desc: "XML → 二进制粒子库",
        params: &[
            file_in("input", "XML 文件", "选择解码出的 XML"),
            choice(
                "version",
                "目标版本",
                "PC / Phone32 / Phone64",
                &["pc", "phone32", "phone64"],
                "pc",
            ),
            file_out("output", "输出粒子库", "留空自动命名"),
        ],
    },
    // ---------------------------------------------------------------- BNK
    Command {
        id: "bnk.extract",
        name: "BNK 音频导出",
        category: "BNK 音频库",
        desc: "解析 .bnk 音频库，导出全部内嵌 WEM 与资源信息",
        params: &[
            file_in("input", "BNK 文件", "选择 .bnk 文件"),
            dir_out("output", "输出目录", "WEM 输出位置"),
        ],
    },
    // ---------------------------------------------------------------- WEM
    Command {
        id: "wem.decode",
        name: "WEM 转音频",
        category: "WEM 音频",
        desc: "把 Wwise 的 .wem 转成可播放的 .wav",
        params: &[
            file_in("input", "WEM 文件", "选择 .wem 文件（Vorbis/AAC/ADPCM/PCM）"),
            file_out("output", "输出 WAV", "留空自动命名"),
        ],
    },
    Command {
        id: "wem.encode",
        name: "音频转 WEM",
        category: "WEM 音频",
        desc: "把 WAV/OGG 编码回 Wwise .wem（替换包内音效用）",
        params: &[
            file_in("input", "音频文件", "选择 .wav（整型 PCM 8-32bit / 16bit ADPCM）或 .ogg（Vorbis）"),
            choice(
                "codec",
                "编码方式",
                "pcm：WAV 整型采样直封；adpcm：WAV 16bit 压缩；vorbis：OGG 直封（PvZ2 官方格式）",
                &["pcm", "adpcm", "vorbis"],
                "pcm",
            ),
            file_out("output", "输出 WEM", "留空自动命名"),
        ],
    },
];

/// 按 ID 查找。
pub fn find(id: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.id == id)
}

/// 按关键字过滤。
pub fn filter(query: &str, category: &str) -> Vec<&'static Command> {
    COMMANDS
        .iter()
        .filter(|c| (category == "全部" || c.category == category) && c.matches(query))
        .collect()
}

/// 出现过的分类（保持表中顺序）。
pub fn used_categories() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for c in COMMANDS {
        if !out.contains(&c.category) {
            out.push(c.category);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<&str> = COMMANDS.iter().map(|c| c.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "存在重复的命令 ID");
    }

    #[test]
    fn covers_14_formats() {
        let cats = used_categories();
        assert_eq!(cats.len(), 14, "应有 14 个格式分类，实际：{cats:?}");
    }

    #[test]
    fn filter_works() {
        assert!(!filter("rsb", "全部").is_empty());
        assert!(filter("不存在的功能xyz", "全部").is_empty());
        // 分类过滤：RTON 分类下只应有 rton 命令
        for c in filter("", "RTON 数据") {
            assert!(c.id.starts_with("rton."));
        }
    }
}
