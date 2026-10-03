// ============================================================================
// commands/mod.rs —— 命令分发
//
// catalog.rs 里的功能表是纯数据；真正干活的实现按格式分四个模块：
//   * archive —— RSB / RSGP / RSBP 补丁 / PAK / DZip
//   * data    —— RTON / SMF / 编译文本 / Crypt-Data / Newton
//   * anim    —— PAM / ReAnim / 粒子特效
//   * audio   —— BNK / WEM
//
// 处理层全部来自 LambdaEd1th/ed1ths-pvz-toolkit 的格式库（AGPL-3.0-or-later）。
// ============================================================================

use crate::params::Params;
use anyhow::{bail, Result};

pub mod anim;
pub mod archive;
pub mod audio;
pub mod data;

/// 按命令 ID 分发到对应实现。
///
/// 命令是同步阻塞函数（可能跑几十秒），调用方（runner）负责丢到后台线程。
pub fn dispatch(id: &str, ps: &Params) -> Result<String> {
    match id {
        // ---- 资源包 ----
        "rsb.unpack" => archive::rsb_unpack(ps),
        "rsb.pack" => archive::rsb_pack(ps),
        "rsb.ptx-export" => archive::rsb_ptx_export(ps),
        "ptx.decode" => archive::ptx_decode(ps),
        "ptx.encode" => archive::ptx_encode(ps),
        "rsgp.unpack" => archive::rsgp_unpack(ps),
        "rsgp.pack" => archive::rsgp_pack(ps),
        "rsbp.create" => archive::rsbp_create(ps),
        "rsbp.apply" => archive::rsbp_apply(ps),
        "pak.unpack" => archive::pak_unpack(ps),
        "pak.pack" => archive::pak_pack(ps),
        "pak.list" => archive::pak_list(ps),
        "dzip.unpack" => archive::dzip_unpack(ps),
        "dzip.pack" => archive::dzip_pack(ps),

        // ---- 数据 ----
        "rton.decode" => data::rton_decode(ps),
        "rton.encode" => data::rton_encode(ps),
        "smf.decode" => data::smf_decode(ps),
        "smf.encode" => data::smf_encode(ps),
        "ctext.decode" => data::ctext_decode(ps),
        "ctext.encode" => data::ctext_encode(ps),
        "cdat.decode" => data::cdat_decode(ps),
        "cdat.encode" => data::cdat_encode(ps),
        "newton.decode" => data::newton_decode(ps),
        "newton.encode" => data::newton_encode(ps),

        // ---- 动画 ----
        "pam.decode" => anim::pam_decode(ps),
        "pam.encode" => anim::pam_encode(ps),
        "reanim.decode" => anim::reanim_decode(ps),
        "reanim.encode" => anim::reanim_encode(ps),
        "reanim.xfl-decode" => anim::reanim_xfl_decode(ps),
        "reanim.xfl-encode" => anim::reanim_xfl_encode(ps),
        "popfx.decode" => anim::popfx_decode(ps),
        "popfx.encode" => anim::popfx_encode(ps),

        // ---- 音频 ----
        "bnk.extract" => audio::bnk_extract(ps),
        "wem.decode" => audio::wem_decode(ps),
        "wem.encode" => audio::wem_encode(ps),

        _ => bail!("未知命令：{id}"),
    }
}
