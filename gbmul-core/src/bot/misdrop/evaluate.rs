//! Single authority for lock-vs-intent comparison (Phase 1).

use super::MisdropReason;

/// Planned lock the bot intended to reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockExpectation {
    pub want_col: usize,
    pub want_rot: usize,
    pub want_row: Option<i32>,
    pub mtype_for_row: &'static str,
}

/// Observed piece pose at a lock check site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockActual {
    pub col: usize,
    pub rot: usize,
    /// Row used for normal-move row comparison (effective lock row).
    pub eff_row: i32,
}

/// Whether begin_drop should defer verify when the piece is still airborne.
pub fn misdrop_check_row(mtype: &str, path: &[String]) -> bool {
    mtype != "normal" || !path.is_empty()
}

/// Map stored intended mtype + executed path terminal mtype (effective-lock-row class).
pub fn resolve_mtype_for_row(il_mtype: &str, path_mtype: &str) -> &'static str {
    if il_mtype == "tuck" && path_mtype == "spin" {
        "spin"
    } else if il_mtype == "tuck" {
        "tuck"
    } else if il_mtype == "spin" {
        "spin"
    } else if il_mtype == "tspin" {
        "tspin"
    } else if path_mtype == "tuck" {
        "tuck"
    } else if path_mtype == "spin" {
        "spin"
    } else {
        "normal"
    }
}

/// Returns `Some(reason)` when col/rot/row disagree with intent.
pub fn evaluate_lock(
    expectation: &LockExpectation,
    actual: &LockActual,
    _path: &[String],
) -> Option<MisdropReason> {
    let col_rot_mismatch =
        actual.col != expectation.want_col || actual.rot != expectation.want_rot;
    if col_rot_mismatch {
        return Some(MisdropReason::LockColRotMismatch {
            want_col: expectation.want_col,
            want_rot: expectation.want_rot,
            got_col: actual.col,
            got_rot: actual.rot,
        });
    }

    // Sprite row at begin_drop can read min_row while BFS anchor row differs (tuck terminal
    // slides). Board post-frame verify is authoritative — defer there via should_defer_row_verify.
    if expectation.mtype_for_row == "tuck" {
        return None;
    }

    if let Some(wr) = expectation.want_row {
        let row_mismatch = match expectation.mtype_for_row {
            // Spin: landed above plan row (S→O on T ledge). Gravity settle +1 is OK.
            "spin" | "tspin" => actual.eff_row < wr,
            // Normal: sprite can read one row deep at begin_drop (partial merge timing).
            "normal" => actual.eff_row < wr || actual.eff_row > wr + 1,
            _ => actual.eff_row != wr,
        };
        if row_mismatch {
            return Some(MisdropReason::LockRowMismatch {
                want_row: wr,
                got_row: actual.eff_row,
            });
        }
    }

    None
}

/// Board-diff lock pose — strict row for spin/tspin (no +1 gravity tolerance).
pub fn evaluate_board_lock(
    expectation: &LockExpectation,
    actual: &LockActual,
    _path: &[String],
) -> Option<MisdropReason> {
    let col_rot_mismatch =
        actual.col != expectation.want_col || actual.rot != expectation.want_rot;
    if col_rot_mismatch {
        return Some(MisdropReason::LockColRotMismatch {
            want_col: expectation.want_col,
            want_rot: expectation.want_rot,
            got_col: actual.col,
            got_rot: actual.rot,
        });
    }
    if let Some(wr) = expectation.want_row {
        if actual.eff_row != wr {
            return Some(MisdropReason::LockRowMismatch {
                want_row: wr,
                got_row: actual.eff_row,
            });
        }
    }
    None
}

/// Grounded begin_drop: col/rot matched but row is not trustworthy yet.
/// Tuck: row settles after terminal slides. Normal: bot ticks pre-frame so row can read
/// one step ahead of the rendered lock — defer to verify_pending_lock at actual lock.
pub fn should_defer_row_verify(
    expectation: &LockExpectation,
    actual: &LockActual,
) -> bool {
    let col_rot_match =
        actual.col == expectation.want_col && actual.rot == expectation.want_rot;
    if !col_rot_match {
        return false;
    }
    let row_diff = expectation
        .want_row
        .is_some_and(|wr| actual.eff_row != wr);
    if !row_diff {
        return false;
    }
    matches!(expectation.mtype_for_row, "tuck" | "normal")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn col_rot_mismatch_is_authoritative() {
        let exp = LockExpectation {
            want_col: 3,
            want_rot: 1,
            want_row: Some(11),
            mtype_for_row: "tuck",
        };
        let act = LockActual {
            col: 6,
            rot: 0,
            eff_row: 10,
        };
        assert!(matches!(
            evaluate_lock(&exp, &act, &[]),
            Some(MisdropReason::LockColRotMismatch { .. })
        ));
    }

    #[test]
    fn spin_row_short_detected() {
        let exp = LockExpectation {
            want_col: 5,
            want_rot: 2,
            want_row: Some(16),
            mtype_for_row: "spin",
        };
        let act = LockActual {
            col: 5,
            rot: 2,
            eff_row: 14,
        };
        assert!(matches!(
            evaluate_lock(&exp, &act, &["D".into()]),
            Some(MisdropReason::LockRowMismatch {
                want_row: 16,
                got_row: 14
            })
        ));
    }

    #[test]
    fn tuck_row_diff_ok_when_col_rot_match() {
        let exp = LockExpectation {
            want_col: 5,
            want_rot: 1,
            want_row: Some(14),
            mtype_for_row: resolve_mtype_for_row("normal", "tuck"),
        };
        let act = LockActual {
            col: 5,
            rot: 1,
            eff_row: 16,
        };
        let path: Vec<String> = std::iter::repeat_n("D".into(), 17)
            .chain(["L".into(), "L".into()])
            .collect();
        assert_eq!(resolve_mtype_for_row("normal", "tuck"), "tuck");
        assert_eq!(evaluate_lock(&exp, &act, &path), None);
        assert!(should_defer_row_verify(&exp, &act));
    }

    #[test]
    fn board_lock_tuck_strict_row() {
        let exp = LockExpectation {
            want_col: 5,
            want_rot: 0,
            want_row: Some(16),
            mtype_for_row: "tuck",
        };
        let act = LockActual {
            col: 5,
            rot: 0,
            eff_row: 15,
        };
        assert!(matches!(
            evaluate_board_lock(&exp, &act, &[]),
            Some(MisdropReason::LockRowMismatch {
                want_row: 16,
                got_row: 15
            })
        ));
        let ok = LockActual {
            col: 5,
            rot: 0,
            eff_row: 16,
        };
        assert_eq!(evaluate_board_lock(&exp, &ok, &[]), None);
    }

    #[test]
    fn board_lock_spin_strict_row() {
        let exp = LockExpectation {
            want_col: 5,
            want_rot: 2,
            want_row: Some(16),
            mtype_for_row: "spin",
        };
        let act = LockActual {
            col: 5,
            rot: 2,
            eff_row: 15,
        };
        assert!(matches!(
            evaluate_board_lock(&exp, &act, &[]),
            Some(MisdropReason::LockRowMismatch {
                want_row: 16,
                got_row: 15
            })
        ));
        assert!(matches!(
            evaluate_lock(&exp, &act, &[]),
            Some(MisdropReason::LockRowMismatch { .. })
        ));
        let exp_deep = LockExpectation {
            want_col: 1,
            want_rot: 0,
            want_row: Some(15),
            mtype_for_row: "spin",
        };
        let deep = LockActual {
            col: 1,
            rot: 0,
            eff_row: 16,
        };
        assert_eq!(evaluate_lock(&exp_deep, &deep, &[]), None, "sprite +1 settle OK");
        assert!(matches!(
            evaluate_board_lock(&exp_deep, &deep, &[]),
            Some(MisdropReason::LockRowMismatch {
                want_row: 15,
                got_row: 16
            })
        ));
    }

    #[test]
    fn spin_row_deep_settle_not_misdrop() {
        let exp = LockExpectation {
            want_col: 1,
            want_rot: 0,
            want_row: Some(15),
            mtype_for_row: "spin",
        };
        let act = LockActual {
            col: 1,
            rot: 0,
            eff_row: 16,
        };
        assert_eq!(evaluate_lock(&exp, &act, &[]), None);
    }

    #[test]
    fn normal_row_mismatch_counts() {
        let exp = LockExpectation {
            want_col: 2,
            want_rot: 0,
            want_row: Some(16),
            mtype_for_row: "normal",
        };
        let act = LockActual {
            col: 2,
            rot: 0,
            eff_row: 14,
        };
        assert!(matches!(
            evaluate_lock(&exp, &act, &["D".into()]),
            Some(MisdropReason::LockRowMismatch {
                want_row: 16,
                got_row: 14
            })
        ));
    }

    #[test]
    fn normal_row_only_diff_defers_at_begin_drop() {
        let exp = LockExpectation {
            want_col: 7,
            want_rot: 1,
            want_row: Some(15),
            mtype_for_row: "normal",
        };
        let act = LockActual {
            col: 7,
            rot: 1,
            eff_row: 16,
        };
        assert!(should_defer_row_verify(&exp, &act));
        assert_eq!(
            evaluate_lock(&exp, &act, &[]),
            None,
            "normal +1 sprite row is deferred, not a misdrop"
        );
    }

    #[test]
    fn normal_col_rot_mismatch_does_not_defer() {
        let exp = LockExpectation {
            want_col: 3,
            want_rot: 1,
            want_row: Some(15),
            mtype_for_row: "normal",
        };
        let act = LockActual {
            col: 2,
            rot: 2,
            eff_row: 16,
        };
        assert!(!should_defer_row_verify(&exp, &act));
        assert!(matches!(
            evaluate_lock(&exp, &act, &[]),
            Some(MisdropReason::LockColRotMismatch { .. })
        ));
    }
}