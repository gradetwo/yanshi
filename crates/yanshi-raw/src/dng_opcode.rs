//! DNG OpcodeList2 解析（DNG 规范 1.7 第 7 章，第四包）：只解析字节
//! 结构，不碰解码器——字节由 `decode` 经 rawler 的 IFD 接口取得，
//! 解析规则在本文件按规范原文实现：
//! - 列表恒大端：u32 opcode 数；每条 = u32 ID + u32 版本 + u32 flags
//!   + u32 参数字节数 + 参数区。
//! - 本引擎执行的只有两项：WarpRectilinear（ID 1，畸变 + 横向色差）
//!   与 FixVignetteRadial（ID 3，暗角增益）。其余 ID 逐项记名跳过
//!   （坏点修复/GainMap 等不在本包范围），不静默吞。
//! - WarpRectilinear2（ID 14）本包不执行：按规范的 skip rule，它后面
//!   跟随的基线 WarpRectilinear 才是给旧读者的等价参数——但若文件
//!   只有 2 代没有基线，本引擎如实跳过整项并留痕，不假装支持。
//!
//! 参数形态（规范原文）：WarpRectilinear = u32 N + N 组
//! (kr0..kr3, kt0, kt1) f64 + (cx̂, ĉy) f64；N=1 全平面共用、
//! N=平面数时逐平面。FixVignetteRadial = (k0..k4, cx̂, ĉy) f64。

use crate::frame::{FileLensCorrection, WarpRectilinearSet};

/// Opcode ID：WarpRectilinear（DNG 1.3）。
pub const OP_WARP_RECTILINEAR: u32 = 1;
/// Opcode ID：FixVignetteRadial（DNG 1.3）。
pub const OP_FIX_VIGNETTE_RADIAL: u32 = 3;
/// Opcode ID：WarpRectilinear2（DNG 1.6，本包不执行、只认名留痕）。
pub const OP_WARP_RECTILINEAR_2: u32 = 14;

fn be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn be_f64(bytes: &[u8], offset: usize) -> Option<f64> {
    let slice = bytes.get(offset..offset + 8)?;
    Some(f64::from_be_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]))
}

/// 解析 OpcodeList2 字节。结构损坏（截断/计数对不上）⇒ 返回已解析
/// 部分 + 注记说明在哪坏的——文件自带校正是逐项优先的数据源，
/// 坏一半不许装作没这回事，也不许让整张图导入失败。
pub fn parse_opcode_list2(bytes: &[u8]) -> (Option<FileLensCorrection>, Vec<String>) {
    let mut notes = Vec::new();
    let Some(count) = be_u32(bytes, 0) else {
        return (
            None,
            vec!["OpcodeList2 字节不足 4 字节，已忽略".to_string()],
        );
    };
    let mut out = FileLensCorrection {
        warp_sets: None,
        warp_center: None,
        vignette_k: None,
        vignette_center: None,
        skipped_opcodes: Vec::new(),
    };
    let mut offset = 4usize;
    let mut saw_baseline_warp_after_v2 = false;
    let mut prev_was_v2 = false;
    for index in 0..count {
        let (Some(id), Some(_version), Some(_flags), Some(param_len)) = (
            be_u32(bytes, offset),
            be_u32(bytes, offset + 4),
            be_u32(bytes, offset + 8),
            be_u32(bytes, offset + 12),
        ) else {
            notes.push(format!(
                "OpcodeList2 第 {} 条头部截断，余下条目未解析",
                index + 1
            ));
            break;
        };
        let param_start = offset + 16;
        let Some(params) = bytes.get(param_start..param_start + param_len as usize) else {
            notes.push(format!(
                "OpcodeList2 第 {} 条（ID {id}）参数区截断，已跳过该条",
                index + 1
            ));
            break;
        };
        match id {
            OP_WARP_RECTILINEAR => {
                // 规范 skip rule 的镜像：本引擎不执行 V2，若 V2 在场且
                // 本条紧跟其后，本条正是给旧读者的基线等价参数——
                // 照常采用（我们就是「旧读者」口径）。
                if prev_was_v2 {
                    saw_baseline_warp_after_v2 = true;
                }
                match parse_warp_rectilinear(params) {
                    Some((sets, center)) => {
                        if out.warp_sets.is_some() {
                            notes.push(
                                "OpcodeList2 含多条 WarpRectilinear：多重 warp 复合不在本包范围，仅采用第一条".to_string(),
                            );
                            out.skipped_opcodes.push(id);
                        } else {
                            out.warp_sets = Some(sets);
                            out.warp_center = Some(center);
                        }
                    }
                    None => {
                        notes.push("WarpRectilinear 参数形态非法，已跳过该条".to_string());
                        out.skipped_opcodes.push(id);
                    }
                }
            }
            OP_FIX_VIGNETTE_RADIAL => match parse_fix_vignette(params) {
                Some((k, center)) => {
                    if out.vignette_k.is_some() {
                        notes
                            .push("OpcodeList2 含多条 FixVignetteRadial：仅采用第一条".to_string());
                        out.skipped_opcodes.push(id);
                    } else {
                        out.vignette_k = Some(k);
                        out.vignette_center = Some(center);
                    }
                }
                None => {
                    notes.push("FixVignetteRadial 参数形态非法，已跳过该条".to_string());
                    out.skipped_opcodes.push(id);
                }
            },
            other => {
                if !out.skipped_opcodes.contains(&other) {
                    out.skipped_opcodes.push(other);
                }
                if other == OP_WARP_RECTILINEAR_2 {
                    notes.push(
                        "OpcodeList2 含 WarpRectilinear2（ID 14）：本包不执行该 opcode，若其后无基线 WarpRectilinear，畸变/色差以镜头库为准".to_string(),
                    );
                }
            }
        }
        prev_was_v2 = id == OP_WARP_RECTILINEAR_2;
        offset = param_start + param_len as usize;
    }
    let _ = saw_baseline_warp_after_v2;
    let has_any = out.warp_sets.is_some() || out.vignette_k.is_some();
    (has_any.then_some(out), notes)
}

/// WarpRectilinear 参数区解析：u32 N + N×6 f64 + 2 f64。
fn parse_warp_rectilinear(params: &[u8]) -> Option<(Vec<WarpRectilinearSet>, [f32; 2])> {
    let n = be_u32(params, 0)? as usize;
    if !(n == 1 || n == 3) {
        return None;
    }
    let expected = 4 + n * 48 + 16;
    if params.len() < expected {
        return None;
    }
    let mut sets = Vec::with_capacity(n);
    for i in 0..n {
        let base = 4 + i * 48;
        let mut vals = [0.0f32; 6];
        for (j, v) in vals.iter_mut().enumerate() {
            *v = be_f64(params, base + j * 8)? as f32;
        }
        sets.push(WarpRectilinearSet {
            kr: [vals[0], vals[1], vals[2], vals[3]],
            kt: [vals[4], vals[5]],
        });
    }
    let center_base = 4 + n * 48;
    let center = [
        be_f64(params, center_base)? as f32,
        be_f64(params, center_base + 8)? as f32,
    ];
    Some((sets, center))
}

/// FixVignetteRadial 参数区解析：7 个 f64。
fn parse_fix_vignette(params: &[u8]) -> Option<([f32; 5], [f32; 2])> {
    if params.len() < 56 {
        return None;
    }
    let mut vals = [0.0f32; 7];
    for (j, v) in vals.iter_mut().enumerate() {
        *v = be_f64(params, j * 8)? as f32;
    }
    Some((
        [vals[0], vals[1], vals[2], vals[3], vals[4]],
        [vals[5], vals[6]],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_u32(buf: &mut Vec<u8>, v: u32) {
        buf.extend_from_slice(&v.to_be_bytes());
    }
    fn push_f64(buf: &mut Vec<u8>, v: f64) {
        buf.extend_from_slice(&v.to_be_bytes());
    }

    #[test]
    fn parses_warp_and_vignette_and_records_skips() {
        let mut buf = Vec::new();
        push_u32(&mut buf, 3);
        // WarpRectilinear, N=1
        let mut params = Vec::new();
        push_u32(&mut params, 1);
        for v in [1.01, -0.02, 0.003, 0.0, 0.001, -0.001] {
            push_f64(&mut params, v);
        }
        push_f64(&mut params, 0.5);
        push_f64(&mut params, 0.5);
        push_u32(&mut buf, OP_WARP_RECTILINEAR);
        push_u32(&mut buf, 0x0103_0000);
        push_u32(&mut buf, 0);
        push_u32(&mut buf, params.len() as u32);
        buf.extend_from_slice(&params);
        // FixVignetteRadial
        let mut params = Vec::new();
        for v in [0.3, -0.05, 0.01, 0.0, 0.0, 0.5, 0.5] {
            push_f64(&mut params, v);
        }
        push_u32(&mut buf, OP_FIX_VIGNETTE_RADIAL);
        push_u32(&mut buf, 0x0103_0000);
        push_u32(&mut buf, 0);
        push_u32(&mut buf, params.len() as u32);
        buf.extend_from_slice(&params);
        // GainMap (ID 9)：不执行、记名跳过。
        push_u32(&mut buf, 9);
        push_u32(&mut buf, 0x0103_0000);
        push_u32(&mut buf, 1);
        push_u32(&mut buf, 0);

        let (parsed, notes) = parse_opcode_list2(&buf);
        let parsed = parsed.expect("两项应解析出来");
        let sets = parsed.warp_sets.unwrap();
        assert_eq!(sets.len(), 1);
        assert!((sets[0].kr[0] - 1.01).abs() < 1e-6);
        assert_eq!(parsed.warp_center, Some([0.5, 0.5]));
        assert_eq!(parsed.vignette_k, Some([0.3, -0.05, 0.01, 0.0, 0.0]));
        assert_eq!(parsed.skipped_opcodes, vec![9]);
        assert!(notes.is_empty(), "正常路径不该有注记：{notes:?}");
    }

    #[test]
    fn truncated_list_notes_instead_of_panicking() {
        let mut buf = Vec::new();
        push_u32(&mut buf, 2);
        push_u32(&mut buf, OP_WARP_RECTILINEAR);
        // 后续字节缺失 ⇒ 注记 + 已解析部分（此处为无）。
        let (parsed, notes) = parse_opcode_list2(&buf);
        assert!(parsed.is_none());
        assert_eq!(notes.len(), 1);
    }
}
