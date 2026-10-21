//! **SVG `d` 字符串语法糖**（曲线/路径二期 §9）。
//!
//! tools 层把 `d` 解析成与 draw_shape 完全同构的 subpaths JSON，再交给
//! 渲染层的同一份校验（`object::parse_path_subpaths`）——本模块只做
//! 「字符串 → 结构」的语法层转换，**不做第二套语义校验**（段数、坐标
//! 有限性等语义错误仍由那一份校验报出）。
//!
//! 支持的命令：`M/m`（moveto，其后坐标对隐含 lineto）、`L/l`、`C/c`、
//! `Q/q`（按公式转 C：`c1 = P0 + 2/3(Q − P0)`、`c2 = P3 + 2/3(Q − P3)`）、
//! `H/h`、`V/v`（转 L）、`Z/z`。小写为相对命令（坐标相对当前点；C 的
//! 两个控制点与终点都相对该段起点）。数字按 SVG 语法扫描：符号与小数点
//! 可作隐含分隔（`10-20`、`1.5.5` 合法），支持科学计数法。
//! 任何语法错误都返回带**字符下标**的原因；不支持的命令（S/T/A 等）
//! 明确报错，不静默忽略。

use serde_json::{json, Value};

/// 解析 SVG path `d` 字符串 → subpaths JSON 数组
/// （形态与 `object::parse_path_subpaths` 的输入一致）。
pub(super) fn parse_svg_d(d: &str) -> Result<Value, String> {
    let mut parser = DParser {
        chars: d.chars().collect(),
        pos: 0,
    };
    parser.run()
}

/// 一条正在拼的子路径（JSON 形态，与结构化输入同构）。
struct SubBuilder {
    start: (f64, f64),
    segments: Vec<Value>,
    closed: bool,
}

struct DParser {
    chars: Vec<char>,
    /// 当前字符下标（错误信息里的位置口径）。
    pos: usize,
}

impl DParser {
    fn error(&self, pos: usize, message: &str) -> String {
        format!("d 解析失败 @字符{pos}：{message}")
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_separators(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == ',' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// 扫描一个 SVG 数字；没有数字时返回 None（不推进）。
    fn scan_number(&mut self) -> Option<f64> {
        let start = self.pos;
        let mut i = self.pos;
        let at = |i: usize| self.chars.get(i).copied();
        if matches!(at(i), Some('+') | Some('-')) {
            i += 1;
        }
        let mut digits = 0usize;
        while matches!(at(i), Some(c) if c.is_ascii_digit()) {
            i += 1;
            digits += 1;
        }
        if at(i) == Some('.') {
            i += 1;
            while matches!(at(i), Some(c) if c.is_ascii_digit()) {
                i += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            return None;
        }
        // 指数部：只有「e/E 后跟（可带符号的）数字」时才吃，否则留给命令字母
        //（`e` 本身不是命令，但 `1e` 这类坏串要在这里判掉而不是吞掉 e）。
        if matches!(at(i), Some('e') | Some('E')) {
            let mut j = i + 1;
            if matches!(at(j), Some('+') | Some('-')) {
                j += 1;
            }
            if matches!(at(j), Some(c) if c.is_ascii_digit()) {
                while matches!(at(j), Some(c) if c.is_ascii_digit()) {
                    j += 1;
                }
                i = j;
            } else {
                // 坏指数：整串数字判非法（位置回到数字起点）。
                self.pos = start;
                return None;
            }
        }
        let text: String = self.chars[start..i].iter().collect();
        match text.parse::<f64>() {
            Ok(value) if value.is_finite() => {
                self.pos = i;
                Some(value)
            }
            _ => None,
        }
    }

    /// 读一个必需数字；缺了/坏了就带位置报错。
    fn require_number(&mut self, context: &str) -> Result<f64, String> {
        self.skip_separators();
        let at = self.pos;
        self.scan_number().ok_or_else(|| {
            self.error(
                at,
                &format!("{context}（此处应是一个数字，实际是 {:?}）", self.peek()),
            )
        })
    }

    /// 下一个 token 是否是数字（隐含重复命令的判据）。
    fn next_is_number(&mut self) -> bool {
        self.skip_separators();
        matches!(
            self.peek(),
            Some(c) if c.is_ascii_digit() || c == '+' || c == '-' || c == '.'
        )
    }

    fn run(&mut self) -> Result<Value, String> {
        let mut subpaths: Vec<Value> = Vec::new();
        let mut open: Option<SubBuilder> = None;
        let mut cur = (0.0f64, 0.0f64);
        let mut start = (0.0f64, 0.0f64);
        // 隐含重复用的当前命令（大写字母 + 是否相对）；M 之后置为 L。
        let mut active: Option<(char, bool)> = None;
        let mut seen_moveto = false;

        fn finish(open: &mut Option<SubBuilder>, subpaths: &mut Vec<Value>) {
            if let Some(sub) = open.take() {
                subpaths.push(json!({
                    "start": [sub.start.0, sub.start.1],
                    "closed": sub.closed,
                    "segments": sub.segments,
                }));
            }
        }

        loop {
            self.skip_separators();
            let Some(c) = self.peek() else { break };
            if c.is_ascii_alphabetic() {
                let cmd_pos = self.pos;
                self.pos += 1;
                let relative = c.is_ascii_lowercase();
                let letter = c.to_ascii_uppercase();
                match letter {
                    'Z' => {
                        let Some(mut sub) = open.take() else {
                            return Err(self.error(cmd_pos, "Z 之前没有子路径（先给 M）"));
                        };
                        sub.closed = true;
                        subpaths.push(json!({
                            "start": [sub.start.0, sub.start.1],
                            "closed": true,
                            "segments": sub.segments,
                        }));
                        cur = start;
                        active = None;
                    }
                    'M' => {
                        finish(&mut open, &mut subpaths);
                        let x = self.require_number("M 后面缺坐标")?;
                        let y = self.require_number("M 后面缺坐标")?;
                        let point = if relative {
                            (cur.0 + x, cur.1 + y)
                        } else {
                            (x, y)
                        };
                        cur = point;
                        start = point;
                        seen_moveto = true;
                        open = Some(SubBuilder {
                            start: point,
                            segments: Vec::new(),
                            closed: false,
                        });
                        // M 的隐含重复是 lineto（同相对性）。
                        active = Some(('L', relative));
                    }
                    'L' | 'C' | 'Q' | 'H' | 'V' => {
                        if !seen_moveto {
                            return Err(self.error(cmd_pos, "d 必须以 M/m 开头"));
                        }
                        active = Some((letter, relative));
                        self.read_group(
                            letter, relative, &mut open, &mut cur, &mut start, cmd_pos,
                        )?;
                    }
                    other => {
                        return Err(self.error(
                            cmd_pos,
                            &format!("不支持的命令 {other}（只支持 M/L/C/Q/H/V/Z 及小写相对形式）"),
                        ));
                    }
                }
            } else if c.is_ascii_digit() || c == '+' || c == '-' || c == '.' {
                if !seen_moveto {
                    return Err(self.error(self.pos, "d 必须以 M/m 开头"));
                }
                let Some((letter, relative)) = active else {
                    return Err(self.error(self.pos, "数字前面没有命令（Z 之后必须重新给命令）"));
                };
                let at = self.pos;
                self.read_group(letter, relative, &mut open, &mut cur, &mut start, at)?;
            } else {
                return Err(self.error(
                    self.pos,
                    &format!("无法识别的字符 {c:?}（只接受命令字母、数字、逗号与空白）"),
                ));
            }
        }
        finish(&mut open, &mut subpaths);
        if subpaths.is_empty() {
            return Err(self.error(0, "d 是空的（至少要有一条 M 子路径）"));
        }
        Ok(Value::Array(subpaths))
    }

    /// 读一组（可能隐含重复多组）当前命令的参数并落段。
    fn read_group(
        &mut self,
        letter: char,
        relative: bool,
        open: &mut Option<SubBuilder>,
        cur: &mut (f64, f64),
        _start: &mut (f64, f64),
        cmd_pos: usize,
    ) -> Result<(), String> {
        let mut groups = 0usize;
        loop {
            if groups > 0 && !self.next_is_number() {
                break;
            }
            let context = format!("命令 {letter} 的参数不完整");
            match letter {
                'L' => {
                    let x = self.require_number(&context)?;
                    let y = self.require_number(&context)?;
                    let to = if relative {
                        (cur.0 + x, cur.1 + y)
                    } else {
                        (x, y)
                    };
                    push_segment(open, json!({"op": "L", "to": [to.0, to.1]}), cmd_pos, self)?;
                    *cur = to;
                }
                'H' => {
                    let x = self.require_number(&context)?;
                    let to = if relative {
                        (cur.0 + x, cur.1)
                    } else {
                        (x, cur.1)
                    };
                    push_segment(open, json!({"op": "L", "to": [to.0, to.1]}), cmd_pos, self)?;
                    *cur = to;
                }
                'V' => {
                    let y = self.require_number(&context)?;
                    let to = if relative {
                        (cur.0, cur.1 + y)
                    } else {
                        (cur.0, y)
                    };
                    push_segment(open, json!({"op": "L", "to": [to.0, to.1]}), cmd_pos, self)?;
                    *cur = to;
                }
                'C' => {
                    let x1 = self.require_number(&context)?;
                    let y1 = self.require_number(&context)?;
                    let x2 = self.require_number(&context)?;
                    let y2 = self.require_number(&context)?;
                    let x = self.require_number(&context)?;
                    let y = self.require_number(&context)?;
                    let (c1, c2, to) = if relative {
                        (
                            (cur.0 + x1, cur.1 + y1),
                            (cur.0 + x2, cur.1 + y2),
                            (cur.0 + x, cur.1 + y),
                        )
                    } else {
                        ((x1, y1), (x2, y2), (x, y))
                    };
                    push_segment(
                        open,
                        json!({"op": "C", "c1": [c1.0, c1.1], "c2": [c2.0, c2.1], "to": [to.0, to.1]}),
                        cmd_pos,
                        self,
                    )?;
                    *cur = to;
                }
                'Q' => {
                    let qx = self.require_number(&context)?;
                    let qy = self.require_number(&context)?;
                    let x = self.require_number(&context)?;
                    let y = self.require_number(&context)?;
                    let (q, to) = if relative {
                        ((cur.0 + qx, cur.1 + qy), (cur.0 + x, cur.1 + y))
                    } else {
                        ((qx, qy), (x, y))
                    };
                    // 二次 → 三次升阶：c1 = P0 + 2/3(Q − P0)，c2 = P3 + 2/3(Q − P3)。
                    let p0 = *cur;
                    let c1 = (
                        p0.0 + 2.0 / 3.0 * (q.0 - p0.0),
                        p0.1 + 2.0 / 3.0 * (q.1 - p0.1),
                    );
                    let c2 = (
                        to.0 + 2.0 / 3.0 * (q.0 - to.0),
                        to.1 + 2.0 / 3.0 * (q.1 - to.1),
                    );
                    push_segment(
                        open,
                        json!({"op": "C", "c1": [c1.0, c1.1], "c2": [c2.0, c2.1], "to": [to.0, to.1]}),
                        cmd_pos,
                        self,
                    )?;
                    *cur = to;
                }
                _ => unreachable!("read_group 只被 L/C/Q/H/V 调用"),
            }
            groups += 1;
        }
        if groups == 0 {
            return Err(self.error(cmd_pos, &format!("命令 {letter} 缺参数")));
        }
        Ok(())
    }
}

/// 把一段推进当前子路径（没有打开的子路径 ⇒ 内部错误路径，带位置报错）。
fn push_segment(
    open: &mut Option<SubBuilder>,
    segment: Value,
    cmd_pos: usize,
    parser: &DParser,
) -> Result<(), String> {
    match open {
        Some(sub) => {
            sub.segments.push(segment);
            Ok(())
        }
        None => Err(parser.error(cmd_pos, "画段之前没有子路径（先给 M）")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(d: &str) -> Value {
        parse_svg_d(d).expect("应当解析成功")
    }

    #[test]
    fn absolute_moveto_lineto_close() {
        let subs = parse("M 10 20 L 30 40 Z");
        assert_eq!(subs.as_array().unwrap().len(), 1);
        let sub = &subs[0];
        assert_eq!(sub["start"], json!([10.0, 20.0]));
        assert_eq!(sub["closed"], json!(true));
        assert_eq!(sub["segments"], json!([{"op": "L", "to": [30.0, 40.0]}]));
    }

    #[test]
    fn implicit_lineto_after_moveto_and_relative_commands() {
        // M 后坐标对隐含 L；小写是相对命令。
        let subs = parse("m 10 10 20 0 l 5 5");
        let sub = &subs[0];
        assert_eq!(sub["start"], json!([10.0, 10.0]));
        assert_eq!(
            sub["segments"],
            json!([
                {"op": "L", "to": [30.0, 10.0]},
                {"op": "L", "to": [35.0, 15.0]},
            ])
        );
        assert_eq!(sub["closed"], json!(false));
    }

    #[test]
    fn h_v_become_l_and_repeat_implicitly() {
        let subs = parse("M 0 0 H 10 20 V 30");
        let sub = &subs[0];
        assert_eq!(
            sub["segments"],
            json!([
                {"op": "L", "to": [10.0, 0.0]},
                {"op": "L", "to": [20.0, 0.0]},
                {"op": "L", "to": [20.0, 30.0]},
            ])
        );
    }

    #[test]
    fn quadratic_is_promoted_with_the_svg_formula() {
        // P0=(0,0)，Q=(30,60)，P3=(90,0)：c1 = (20,40)，c2 = (50,40)。
        let subs = parse("M 0 0 Q 30 60 90 0");
        let seg = &subs[0]["segments"][0];
        assert_eq!(seg["op"], json!("C"));
        assert_eq!(seg["c1"], json!([20.0, 40.0]));
        assert_eq!(seg["c2"], json!([50.0, 40.0]));
        assert_eq!(seg["to"], json!([90.0, 0.0]));
    }

    #[test]
    fn relative_cubic_offsets_from_segment_start() {
        let subs = parse("M 100 100 c 10 0 20 10 30 10");
        let seg = &subs[0]["segments"][0];
        assert_eq!(seg["c1"], json!([110.0, 100.0]));
        assert_eq!(seg["c2"], json!([120.0, 110.0]));
        assert_eq!(seg["to"], json!([130.0, 110.0]));
    }

    #[test]
    fn number_syntax_implicit_separators_and_exponents() {
        let subs = parse("M10-20L1.5.5 2e1 4");
        let sub = &subs[0];
        assert_eq!(sub["start"], json!([10.0, -20.0]));
        assert_eq!(
            sub["segments"],
            json!([
                {"op": "L", "to": [1.5, 0.5]},
                {"op": "L", "to": [20.0, 4.0]},
            ])
        );
    }

    #[test]
    fn multiple_subpaths() {
        let subs = parse("M 0 0 L 1 1 M 5 5 L 6 6 Z");
        let arr = subs.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[1]["closed"], json!(true));
        assert_eq!(arr[1]["start"], json!([5.0, 5.0]));
    }

    #[test]
    fn errors_carry_char_positions() {
        // 不支持的命令 A：第 6 个字符（下标从 0 数）。
        let err = parse_svg_d("M 0 0 A 1 1").unwrap_err();
        assert!(err.contains("@字符6"), "实测：{err}");
        // 数字打头。
        let err = parse_svg_d("10 10").unwrap_err();
        assert!(err.contains("必须以 M/m 开头"), "实测：{err}");
        // C 参数不完整：错在串尾附近，必须带位置。
        let err = parse_svg_d("M 0 0 C 1 2 3").unwrap_err();
        assert!(err.contains("@字符"), "实测：{err}");
        // Z 后直接跟数字。
        let err = parse_svg_d("M 0 0 L 1 1 Z 5").unwrap_err();
        assert!(err.contains("@字符"), "实测：{err}");
        // 空串。
        assert!(parse_svg_d("   ").is_err());
        // 无法识别的字符。
        let err = parse_svg_d("M 0 0 #").unwrap_err();
        assert!(err.contains("@字符6"), "实测：{err}");
    }

    #[test]
    fn moveto_only_subpath_is_left_for_the_shared_validator() {
        // 语法层只管字符串 → 结构；「段数 ≥ 1」是语义校验的活（同一份校验报）。
        let subs = parse("M 3 4");
        assert_eq!(subs[0]["segments"], json!([]));
    }
}
