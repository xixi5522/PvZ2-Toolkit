// ============================================================================
// colors.rs —— Monet 取色引擎
//
// 把「种子色 → 完整 MD3 明/暗调色板」这件 2.0 里删掉的事请回来，
// 但这次不是自研 HSL 近似，而是直接用 Google 官方的
// material-color-utilities（Material You / Monet 的参考实现）：
//   * HCT 色彩空间（CAM16 色度 + CIELAB 明度）—— 与 Android 12+
//     系统取色完全同源
//   * TonalSpot variant —— Pixel 壁纸取色用的默认 variant
//   * Spec 2021 —— 标准 MD3 角色排布（tone 40/80/90/…）
//
// 算出的 30 个角色色注入 fork 样式的 Monet global（ui/widgets/monet.slint），
// 官方 material 组件（fork 版）与自绘骨架同时吃这一套颜色。
//
// 种子色来源（settings.rs 决定）：
//   * 跟随系统莫奈 —— Android 读壁纸主色/系统强调色（platform.rs），
//     桌面回退 Slint 默认强调色（跟官方 backend 注入的值一致）
//   * 自定义 —— 设置页预设色板或手输 hex
// ============================================================================

use crate::ui::{AppWindow, Monet};
use material_color_utilities::dynamiccolor::{DynamicScheme, Platform, SpecVersion, Variant};
use material_color_utilities::hct::Hct;
use material_color_utilities::palettes::TonalPalette;
use slint::ComponentHandle;
use slint::private_unstable_api::re_exports::ColorScheme;

/// 默认种子色 = MD3 baseline 紫 #6750A4（Slint material 官方底色，
/// 与 Android 默认 system_accent 同族）。
pub const DEFAULT_SEED: u32 = 0xFF6750A4;

/// 设置页的预设种子色（MD3 官方示例色系 + 常用主题色）。
/// 前景色配套显示用，读的时候自己按 tone 反推即可，这里只存种子。
pub const COLOR_PRESETS: &[(&str, u32)] = &[
    ("baseline", 0xFF6750A4), // MD3 baseline 紫
    ("蓝", 0xFF1565C0),
    ("青", 0xFF00838F),
    ("绿", 0xFF2E7D32),
    ("橄榄", 0xFF708238),
    ("琥珀", 0xFFB26A00),
    ("橙", 0xFFD84315),
    ("红", 0xFFC62828),
    ("粉", 0xFFAD1457),
    ("紫", 0xFF6A1B9A),
    ("靛蓝", 0xFF283593),
    ("棕", 0xFF6D4C41),
];

/// 解析 #RRGGBB / #AARRGGBB / RRGGBB。
/// 不合法返回 None（调用方回退 DEFAULT_SEED）。
pub fn parse_hex(s: &str) -> Option<u32> {
    let s = s.trim().trim_start_matches('#');
    match s.len() {
        6 => u32::from_str_radix(s, 16).ok().map(|v| 0xFF00_0000 | v),
        8 => u32::from_str_radix(s, 16).ok(),
        _ => None,
    }
}

/// 用 TonalSpot（Pixel 壁纸取色的默认 variant）+ Spec 2021 构造方案。
pub fn build_scheme(seed_argb: u32, dark: bool) -> DynamicScheme {
    let hct = Hct::from_int(seed_argb);
    let hue = hct.hue();

    // —— TonalSpot 的调色板公式（与上游 palettes_spec_2021 一致）——
    let primary = TonalPalette::from_hue_and_chroma(hue, 36.0);
    let secondary = TonalPalette::from_hue_and_chroma(hue, 16.0);
    let tertiary = TonalPalette::from_hue_and_chroma(hue + 60.0, 24.0);
    let neutral = TonalPalette::from_hue_and_chroma(hue, 4.0);
    let neutral_variant = TonalPalette::from_hue_and_chroma(hue, 8.0);
    let error = TonalPalette::from_hue_and_chroma(25.0, 84.0);

    DynamicScheme::new(
        hct,
        Variant::TonalSpot,
        dark,
        Platform::Phone,
        0.0,
        SpecVersion::Spec2021,
        primary,
        secondary,
        tertiary,
        neutral,
        neutral_variant,
        error,
    )
}

// ----------------------------------------------------------------------------
// ★ 上游 bug 绕行（material-color-utilities 1.0.0-dev.18）：
//
//   实测（种 #6750A4、light）scheme.secondary_container() 返回 #4A4458
//   —— 这是官方 dark secondary-container（tone 30）的精确值；dark 模式
//   下则返回 light 的 #E8DEF8（tone 90）。primary/secondary-container、
//   tertiary-container、error-container 八个「container 系」角色全部
//   明暗反转，其余角色（primary/secondary/surface 系）正常。
//
//   根因在库的 ToneDeltaPair/ContrastCurve 求值链（container 色带
//   tone_delta_pair 与 contrast_curve，方向判断用了错误的 is_dark
//   语义），不带 delta 的简单 tone 角色不受影响。
//
//   TonalSpot 非 fidelity，Spec2021 规格里 container 系就是调色板上的
//   固定 tone —— 这里直接按官方表取，绕开坏掉的求值链：
//     * *-container        ：light 90 / dark 30
//     * on-*-container     ：light 10 / dark 90
// ----------------------------------------------------------------------------

/// container 系背景色（*​-container）
fn container_tone(palette: &TonalPalette, dark: bool) -> u32 {
    palette.tone(if dark { 30 } else { 90 })
}

/// container 系前景色（on-*-container）
fn on_container_tone(palette: &TonalPalette, dark: bool) -> u32 {
    palette.tone(if dark { 90 } else { 10 })
}

/// 把整套方案注入 Monet global。官方组件（fork material）与
/// 自绘骨架（Theme 镜像）同时生效。
pub fn inject(window: &AppWindow, seed_argb: u32, dark: bool) {
    let scheme = build_scheme(seed_argb, dark);
    let m = window.global::<Monet>();

    // brush 快捷构造
    let b = |argb: u32| slint::Color::from_argb_encoded(argb).into();

    m.set_dark(dark);
    m.set_color_scheme(if dark { ColorScheme::Dark } else { ColorScheme::Light });
    m.set_seed(b(seed_argb));

    // ---- Neutral / surface 系 ----
    m.set_surface(b(scheme.surface()));
    m.set_surface_dim(b(scheme.surface_dim()));
    m.set_surface_bright(b(scheme.surface_bright()));
    m.set_surface_container_lowest(b(scheme.surface_container_lowest()));
    m.set_surface_container_low(b(scheme.surface_container_low()));
    m.set_surface_container(b(scheme.surface_container()));
    m.set_surface_container_high(b(scheme.surface_container_high()));
    m.set_surface_container_highest(b(scheme.surface_container_highest()));
    m.set_on_surface(b(scheme.on_surface()));
    m.set_on_surface_variant(b(scheme.on_surface_variant()));
    m.set_outline(b(scheme.outline()));
    m.set_outline_variant(b(scheme.outline_variant()));
    m.set_inverse_surface(b(scheme.inverse_surface()));
    m.set_inverse_on_surface(b(scheme.inverse_on_surface()));

    // ---- Primary ----
    let pp = scheme.primary_palette();
    m.set_primary(b(scheme.primary()));
    m.set_on_primary(b(scheme.on_primary()));
    // container 系走修正层（上游 dev.18 明暗反转，见 container_tone 注释）
    m.set_primary_container(b(container_tone(pp, dark)));
    m.set_on_primary_container(b(on_container_tone(pp, dark)));

    // ---- Secondary ----
    let sp = scheme.secondary_palette();
    m.set_secondary(b(scheme.secondary()));
    m.set_on_secondary(b(scheme.on_secondary()));
    m.set_secondary_container(b(container_tone(sp, dark)));
    m.set_on_secondary_container(b(on_container_tone(sp, dark)));

    // ---- Tertiary ----
    let tp = scheme.tertiary_palette();
    m.set_tertiary(b(scheme.tertiary()));
    m.set_tertiary_container(b(container_tone(tp, dark)));
    m.set_on_tertiary_container(b(on_container_tone(tp, dark)));

    // ---- Error ----
    let ep = scheme.error_palette();
    m.set_error(b(scheme.error()));
    m.set_on_error(b(scheme.on_error()));
    m.set_error_container(b(container_tone(ep, dark)));
    m.set_on_error_container(b(on_container_tone(ep, dark)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_seed_keeps_hue_and_tonalspot_shape() {
        // 注意：TonalSpot 的 primary 调色板 chroma 固定为 36，而 MD3 baseline
        // 的 #6750A4 是官方手工调色板（chroma 48）—— 所以 TonalSpot(seed)
        // 的 primary **不该**精确等于 #6750A4（实测 #65558F，属正常）。
        // 这里钉死的是 TonalSpot 的三个不变量：
        //   1. 色相 = 种子色相（±2°，防 hue 计算错位）
        //   2. tone = 40（light primary 规格）
        //   3. chroma = 36（TonalSpot primary 公式）
        let s = build_scheme(DEFAULT_SEED, false);
        let p = Hct::from_int(s.primary());
        let seed_hue = Hct::from_int(DEFAULT_SEED).hue();
        let hue_diff = (p.hue() - seed_hue + 180.0).rem_euclid(360.0) - 180.0;
        assert!(hue_diff.abs() < 2.0, "色相漂移 {hue_diff}°（seed {seed_hue} → primary {}）", p.hue());
        assert!((p.tone() - 40.0).abs() < 2.0, "primary tone {} ≠ 40", p.tone());
        assert!((p.chroma() - 36.0).abs() < 2.0, "primary chroma {} ≠ 36", p.chroma());
    }

    #[test]
    fn container_roles_match_md3_direction() {
        // 回归钉：上游 dev.18 的 ToneDeltaPair/ContrastCurve 明暗反转 bug
        // （light 下 *-container 返回 dark tone30，反之亦然）。修正层
        // container_tone/on_container_tone 必须给出官方方向。
        let l = build_scheme(DEFAULT_SEED, false);
        let d = build_scheme(DEFAULT_SEED, true);
        let lc = Hct::from_int(container_tone(l.primary_palette(), false));
        assert!((lc.tone() - 90.0).abs() < 1.0, "light *-container tone {} ≠ 90", lc.tone());
        let dc = Hct::from_int(container_tone(d.primary_palette(), true));
        assert!((dc.tone() - 30.0).abs() < 1.0, "dark *-container tone {} ≠ 30", dc.tone());
        let lo = Hct::from_int(on_container_tone(l.primary_palette(), false));
        assert!((lo.tone() - 10.0).abs() < 1.0, "light on-*-container tone {} ≠ 10", lo.tone());
        let dco = Hct::from_int(on_container_tone(d.primary_palette(), true));
        assert!((dco.tone() - 90.0).abs() < 1.0, "dark on-*-container tone {} ≠ 90", dco.tone());

        // 官方 baseline light primary-container = #EADDFF（primary 调色板 tone90）
        assert!(
            (Hct::from_int(container_tone(l.primary_palette(), false)).tone() - 90.0).abs() < 1.0
        );
    }

    #[test]
    fn light_scheme_matches_md3_baseline_surface() {
        // MD3 baseline light surface = #FEF7FF（neutral tone 98）
        let s = build_scheme(0xFF6750A4, false);
        let surf = s.surface();
        let ch = |v: u32, shift: u32| ((v >> shift) & 0xFF) as i32;
        assert!(
            (ch(surf, 16) - 0xFE).abs() <= 2
                && (ch(surf, 8) - 0xF7).abs() <= 2
                && (ch(surf, 0) - 0xFF).abs() <= 2,
            "surface {surf:#08x} 不在 baseline 附近"
        );
    }

    #[test]
    fn dark_scheme_is_actually_dark() {
        let l = build_scheme(0xFF6750A4, false);
        let d = build_scheme(0xFF6750A4, true);
        let lum = |v: u32| {
            0.2126 * ((v >> 16) & 0xFF) as f64
                + 0.7152 * ((v >> 8) & 0xFF) as f64
                + 0.0722 * (v & 0xFF) as f64
        };
        assert!(lum(d.surface()) < lum(l.surface()));
        // 深色 primary 应该是亮色（tone 80 系），浅色 primary 是 tone 40 系
        assert!(lum(d.primary()) > lum(l.primary()));
    }

    #[test]
    fn all_roles_are_opaque() {
        for dark in [false, true] {
            let s = build_scheme(0xFF1565C0, dark);
            for (name, argb) in [
                ("primary", s.primary()),
                ("on_primary", s.on_primary()),
                ("surface", s.surface()),
                ("on_surface", s.on_surface()),
                ("surface_container", s.surface_container()),
                ("outline", s.outline()),
                ("error", s.error()),
            ] {
                assert_eq!(argb >> 24, 0xFF, "{name}（dark={dark}）alpha 不是 255");
            }
        }
    }

    #[test]
    fn hex_parsing() {
        assert_eq!(parse_hex("#6750A4"), Some(0xFF6750A4));
        assert_eq!(parse_hex("6750A4"), Some(0xFF6750A4));
        assert_eq!(parse_hex("#FF6750A4"), Some(0xFF6750A4));
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("zzzzzz"), None);
    }
}
