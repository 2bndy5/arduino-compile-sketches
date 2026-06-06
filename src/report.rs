use arduino_report_size_deltas::report_structs::{
    AbsCount, BoardSize, SizeValue, Sketch, SketchDeltaSize, SketchSize, SketchSizeKind,
    SketchWarnings,
};
use regex::Regex;

use crate::error::Result;

pub(super) fn get_sizes_from_output(compilation_output: &str) -> Result<Vec<SketchSizeKind>> {
    let flash_re = Regex::new(
        r"Sketch uses ([0-9]+) bytes .*of program storage space\.?(?: Maximum is ([0-9]+) bytes\.)?",
    )?;
    let ram_re = Regex::new(
        r"Global variables use ([0-9]+) bytes .*of dynamic memory(?:.*\.? Maximum is ([0-9]+) bytes\.)?",
    )?;

    let (flash_abs, flash_max) = flash_re
        .captures(compilation_output)
        .map(|c| {
            let abs = c.get(1).and_then(|m| m.as_str().parse::<i64>().ok());
            let max = c.get(2).and_then(|m| m.as_str().parse::<u64>().ok());
            (abs, max)
        })
        .unwrap_or((None, None));

    let (ram_abs, ram_max) = ram_re
        .captures(compilation_output)
        .map(|c| {
            let abs = c.get(1).and_then(|m| m.as_str().parse::<i64>().ok());
            let max = c.get(2).and_then(|m| m.as_str().parse::<u64>().ok());
            (abs, max)
        })
        .unwrap_or((None, None));

    let size_kinds = vec![
        SketchSizeKind::Flash {
            size: mk_sketch_size_from_captures(flash_max, flash_abs, "flash"),
        },
        SketchSizeKind::Ram {
            size: mk_sketch_size_from_captures(ram_max, ram_abs, "RAM"),
        },
    ];
    Ok(size_kinds)
}

fn mk_sketch_size_from_captures(
    max_value: Option<u64>,
    abs_value: Option<i64>,
    mem_kind: &str,
) -> SketchSize {
    let relative = match (max_value, abs_value) {
        (Some(max), Some(abs)) => Some(SizeValue::Known(abs as f32 / max as f32)),
        _ => None,
    };
    if max_value.is_none() {
        log::warn!(
            "Unable to determine the maximum {mem_kind} size. The board's platform may not have been configured to provide this information.",
        );
    }
    if abs_value.is_none() {
        log::warn!(
            "Unable to determine the absolute {mem_kind} size. The board's platform may not have been configured to provide this information.",
        );
    }
    SketchSize {
        maximum: max_value
            .map(SizeValue::Known)
            .or(Some(SizeValue::NotApplicable)),
        current: SketchDeltaSize {
            absolute: abs_value
                .map(SizeValue::Known)
                .unwrap_or(SizeValue::NotApplicable),
            relative,
        },
        previous: None,
        delta: None,
    }
}

pub(super) fn get_warning_count_from_output(compilation_output: &str) -> Result<SketchWarnings> {
    let warn_re = Regex::new(r"\:[0-9]+\:[0-9]+\: warning\:")?;
    let count = warn_re.find_iter(compilation_output).count();
    Ok(SketchWarnings {
        current: AbsCount {
            absolute: count as i32,
        },
        previous: AbsCount::default(),
        delta: AbsCount::default(),
    })
}

pub(super) fn apply_base_report(sketch_reports: &mut [Sketch], base_sketches: &[Sketch]) {
    for sketch in sketch_reports.iter_mut() {
        let Some(base_sketch) = base_sketches.iter().find(|s| s.name == sketch.name) else {
            continue;
        };

        for size in sketch.sizes.iter_mut() {
            match size {
                SketchSizeKind::Ram { size: current_size } => {
                    for base_size in &base_sketch.sizes {
                        if let SketchSizeKind::Ram { size: base_size } = base_size {
                            calc_deltas(current_size, base_size);
                            break;
                        }
                    }
                }
                SketchSizeKind::Flash { size: current_size } => {
                    for base_size in &base_sketch.sizes {
                        if let SketchSizeKind::Flash { size: base_size } = base_size {
                            calc_deltas(current_size, base_size);
                            break;
                        }
                    }
                }
            }
        }

        if let Some(curr_warnings) = sketch.warnings.as_mut()
            && let Some(base_warnings) = base_sketch.warnings.as_ref()
        {
            curr_warnings.previous = AbsCount {
                absolute: base_warnings.current.absolute,
            };
            curr_warnings.delta = AbsCount {
                absolute: curr_warnings.current.absolute - base_warnings.current.absolute,
            };
        }
    }
}

fn calc_deltas(current_size: &mut SketchSize, base_size: &SketchSize) {
    current_size.previous = Some(SketchDeltaSize {
        absolute: base_size.current.absolute,
        relative: None,
    });

    let (delta_abs, delta_relative) =
        match (&current_size.current.absolute, &base_size.current.absolute) {
            (SizeValue::Known(curr), SizeValue::Known(prev)) => {
                let delta = curr - prev;
                let delta_rel = match current_size.maximum {
                    Some(SizeValue::Known(v)) => Some(SizeValue::Known(delta as f32 / v as f32)),
                    _ => Some(SizeValue::NotApplicable),
                };
                (SizeValue::Known(delta), delta_rel)
            }
            _ => (SizeValue::NotApplicable, Some(SizeValue::NotApplicable)),
        };

    current_size.delta = Some(SketchDeltaSize {
        absolute: delta_abs,
        relative: delta_relative,
    });
}

pub(super) fn get_board_sizes_from_summary(sizes: &[SketchSizeKind]) -> Option<Vec<BoardSize>> {
    let mut board_sizes = Vec::new();
    for size in sizes {
        match size {
            SketchSizeKind::Flash { size } => board_sizes.push(BoardSize::Flash {
                maximum: size.maximum,
            }),
            SketchSizeKind::Ram { size } => board_sizes.push(BoardSize::Ram {
                maximum: size.maximum,
            }),
        }
    }

    if board_sizes.is_empty() {
        None
    } else {
        Some(board_sizes)
    }
}

pub(super) fn get_sizes_summary_report(sketch_reports: &[Sketch]) -> Vec<SketchSizeKind> {
    // Accumulate totals directly into values we will use to build
    // `SketchSizeKind` instances for flash and ram.
    let mut flash_max: Option<SizeValue<u64>> = None;
    let mut flash_current_total: Option<i64> = None;
    let mut flash_previous_total: Option<i64> = None;

    let mut ram_max: Option<SizeValue<u64>> = None;
    let mut ram_current_total: Option<i64> = None;
    let mut ram_previous_total: Option<i64> = None;

    for sketch in sketch_reports {
        for size_kind in &sketch.sizes {
            match size_kind {
                SketchSizeKind::Flash { size } => {
                    accumulate_size(
                        size,
                        &mut flash_max,
                        &mut flash_current_total,
                        &mut flash_previous_total,
                    );
                }
                SketchSizeKind::Ram { size } => {
                    accumulate_size(
                        size,
                        &mut ram_max,
                        &mut ram_current_total,
                        &mut ram_previous_total,
                    );
                }
            }
        }
    }

    let mut output = Vec::new();

    if let Some(size_kind) =
        culminate_sketch_size(flash_max, flash_current_total, flash_previous_total, true)
    {
        output.push(size_kind);
    }

    if let Some(size_kind) =
        culminate_sketch_size(ram_max, ram_current_total, ram_previous_total, false)
    {
        output.push(size_kind);
    }

    output
}

fn accumulate_size(
    size: &SketchSize,
    max: &mut Option<SizeValue<u64>>,
    current_total: &mut Option<i64>,
    previous_total: &mut Option<i64>,
) {
    if max.is_none_or(|v| {
        if let SizeValue::Known(max_known) = v
            && let Some(SizeValue::Known(size_max)) = size.maximum
            && size_max > max_known
        {
            true
        } else {
            false
        }
    }) {
        *max = size.maximum;
    }

    if let SizeValue::Known(value) = size.current.absolute {
        match current_total {
            Some(total) => *total += value,
            None => *current_total = Some(value),
        }
    }

    if let Some(previous) = &size.previous
        && let SizeValue::Known(value) = previous.absolute
    {
        match previous_total {
            Some(total) => *total += value,
            None => *previous_total = Some(value),
        }
    }
}

fn culminate_sketch_size(
    maximum: Option<SizeValue<u64>>,
    current_total: Option<i64>,
    previous_total: Option<i64>,
    is_flash: bool,
) -> Option<SketchSizeKind> {
    if current_total.is_some() || previous_total.is_some() {
        let current = current_total.map(SizeValue::Known).unwrap_or_default();
        let previous = previous_total.map(|total| SketchDeltaSize {
            absolute: SizeValue::Known(total),
            relative: None,
        });
        let delta = if let Some(curr) = current_total
            && let Some(prev) = previous_total
        {
            Some(SketchDeltaSize {
                absolute: SizeValue::Known(curr - prev),
                relative: None,
            })
        } else {
            None
        };

        let sketch_size = SketchSize {
            maximum,
            current: SketchDeltaSize {
                absolute: current,
                relative: None,
            },
            previous,
            delta,
        };

        if is_flash {
            Some(SketchSizeKind::Flash { size: sketch_size })
        } else {
            Some(SketchSizeKind::Ram { size: sketch_size })
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use std::{fs, path::PathBuf};

    use arduino_report_size_deltas::report_structs::{
        AbsCount, SizeValue, Sketch, SketchDeltaSize, SketchSize, SketchSizeKind, SketchWarnings,
    };

    #[test]
    fn get_sizes_from_output() {
        let output = "Sketch uses 802 bytes (1%) of program storage space. Maximum is 1604 bytes.\
            Global variables use 22 bytes (0%) of dynamic memory, leaving 6122 bytes for local variables. Maximum is 33 bytes.";
        let sizes = super::get_sizes_from_output(output).unwrap();
        assert_eq!(sizes.len(), 2);
        for size in &sizes {
            match size {
                SketchSizeKind::Flash { size } => {
                    let Some(SizeValue::Known(max_size)) = size.maximum else {
                        panic!("expected max flash size parsed from output");
                    };
                    assert_eq!(max_size, 1604);
                    let abs_size = match &size.current.absolute {
                        SizeValue::Known(n) => *n,
                        _ => 0_i64,
                    };
                    assert_eq!(abs_size, 802);
                }
                SketchSizeKind::Ram { size } => {
                    let Some(SizeValue::Known(max_size)) = size.maximum else {
                        panic!("expected max ram size parsed from output");
                    };
                    assert_eq!(max_size, 33);
                    let abs_size = match &size.current.absolute {
                        SizeValue::Known(n) => *n,
                        _ => 0_i64,
                    };
                    assert_eq!(abs_size, 22);
                }
            }
        }
    }

    #[test]
    fn get_warning_count_from_output() {
        let test_asset_path = PathBuf::from("tests/warnings_assets/has-warnings.txt");
        let out = fs::read_to_string(&test_asset_path).unwrap();
        let cnt = super::get_warning_count_from_output(&out).unwrap();
        assert_eq!(cnt.current.absolute, 45);

        let no_warnings_asset = test_asset_path.with_file_name("no-warnings.txt");
        let out = fs::read_to_string(&no_warnings_asset).unwrap();
        let cnt = super::get_warning_count_from_output(&out).unwrap();
        assert_eq!(cnt.current.absolute, 0);
    }

    #[test]
    fn get_sizes_summary_report() {
        // two sketches S1 and S2 each include previous values
        let sk1 = Sketch {
            name: "S1".into(),
            compilation_success: true,
            sizes: vec![SketchSizeKind::Flash {
                size: SketchSize {
                    maximum: None,
                    current: SketchDeltaSize {
                        absolute: SizeValue::Known(110),
                        relative: None,
                    },
                    previous: Some(SketchDeltaSize {
                        absolute: SizeValue::Known(100),
                        relative: None,
                    }),
                    delta: None,
                },
            }],
            warnings: None,
        };
        let sk2 = Sketch {
            name: "S2".into(),
            compilation_success: true,
            sizes: vec![SketchSizeKind::Flash {
                size: SketchSize {
                    maximum: None,
                    current: SketchDeltaSize {
                        absolute: SizeValue::Known(90),
                        relative: None,
                    },
                    previous: Some(SketchDeltaSize {
                        absolute: SizeValue::Known(100),
                        relative: None,
                    }),
                    delta: None,
                },
            }],
            warnings: None,
        };

        let sketch_reports = vec![sk1, sk2];

        let res = super::get_sizes_summary_report(&sketch_reports);
        assert_eq!(res.len(), 1);
        match &res[0] {
            SketchSizeKind::Flash { size } => {
                assert!(matches!(size.current.absolute, SizeValue::Known(200)));
                assert!(matches!(
                    size.previous.as_ref().map(|v| &v.absolute),
                    Some(SizeValue::Known(200))
                ));
                assert!(matches!(
                    size.delta.as_ref().map(|v| &v.absolute),
                    Some(SizeValue::Known(0))
                ));
            }
            _ => panic!("expected flash size kind"),
        }
    }

    #[test]
    fn apply_base_report_sets_warnings_delta() {
        let mut sketches = vec![Sketch {
            name: "S1".into(),
            compilation_success: true,
            sizes: vec![],
            warnings: Some(SketchWarnings {
                current: AbsCount { absolute: 5 },
                previous: AbsCount::default(),
                delta: AbsCount::default(),
            }),
        }];

        let base_sketches = vec![Sketch {
            name: "S1".into(),
            compilation_success: true,
            sizes: vec![],
            warnings: Some(SketchWarnings {
                current: AbsCount { absolute: 2 },
                previous: AbsCount::default(),
                delta: AbsCount::default(),
            }),
        }];

        super::apply_base_report(&mut sketches, &base_sketches);
        let warnings = sketches[0].warnings.as_ref().unwrap();
        assert_eq!(warnings.previous.absolute, 2);
        assert_eq!(warnings.delta.absolute, 3);
    }
}
