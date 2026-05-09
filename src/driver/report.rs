use arduino_report_size_deltas::report_structs::{
    AbsCount, BoardSize, Report, SizeValue, Sketch, SketchDeltaSize, SketchSize, SketchSizeKind,
    SketchWarnings,
};
use regex::Regex;

use crate::driver::CompileSketches;
use crate::error::Result;

impl CompileSketches {
    pub(super) fn get_sizes_from_output(
        &self,
        compilation_output: &str,
    ) -> Result<Vec<SketchSizeKind>> {
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
                size: SketchSize {
                    maximum: match flash_max {
                        Some(v) => Some(SizeValue::Known(v)),
                        None => {
                            log::warn!(
                                "Unable to determine the maximum flash size. The board's platform may not have been configured to provide this information."
                            );
                            Some(SizeValue::NotApplicable("N/A".to_string()))
                        }
                    },
                    current: SketchDeltaSize {
                        absolute: match flash_abs {
                            Some(v) => SizeValue::Known(v),
                            None => {
                                log::warn!(
                                    "Unable to determine the absolute flash size. The board's platform may not have been configured to provide this information."
                                );
                                SizeValue::NotApplicable("N/A".to_string())
                            }
                        },
                        relative: None,
                    },
                    previous: None,
                    delta: None,
                },
            },
            SketchSizeKind::Ram {
                size: SketchSize {
                    maximum: match ram_max {
                        Some(v) => Some(SizeValue::Known(v)),
                        None => {
                            log::warn!(
                                "Unable to determine the maximum RAM size. The board's platform may not have been configured to provide this information."
                            );
                            Some(SizeValue::NotApplicable("N/A".to_string()))
                        }
                    },
                    current: SketchDeltaSize {
                        absolute: match ram_abs {
                            Some(v) => SizeValue::Known(v),
                            None => {
                                log::warn!(
                                    "Unable to determine the absolute flash size. The board's platform may not have been configured to provide this information."
                                );
                                SizeValue::NotApplicable("N/A".to_string())
                            }
                        },
                        relative: None,
                    },
                    previous: None,
                    delta: None,
                },
            },
        ];
        Ok(size_kinds)
    }

    pub(super) fn get_warning_count_from_output(
        &self,
        compilation_output: &str,
    ) -> Result<SketchWarnings> {
        let warn_re = Regex::new(r":[0-9]+:[0-9]+: warning:")?;
        let count = warn_re.find_iter(compilation_output).count();
        Ok(SketchWarnings {
            current: AbsCount {
                absolute: count as i32,
            },
            previous: AbsCount::default(),
            delta: AbsCount::default(),
        })
    }

    pub(super) fn apply_base_report(
        &self,
        sketch_reports: &mut [Sketch],
        base_results: Option<&Report>,
    ) {
        let Some(base) = base_results else {
            return;
        };

        let Some(base_board) = base
            .boards
            .iter()
            .find(|b| b.board == self.sketch_compiler.fqbn)
        else {
            return;
        };

        for sketch in sketch_reports.iter_mut() {
            let Some(base_sketch) = base_board.sketches.iter().find(|s| s.name == sketch.name)
            else {
                continue;
            };

            for size in sketch.sizes.iter_mut() {
                let base_size = base_sketch
                    .sizes
                    .iter()
                    .find(|s| Self::same_size_kind(s, size));
                if let Some(base_size) = base_size {
                    Self::apply_size_delta(size, base_size);
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

    fn same_size_kind(left: &SketchSizeKind, right: &SketchSizeKind) -> bool {
        matches!(
            (left, right),
            (SketchSizeKind::Flash { .. }, SketchSizeKind::Flash { .. })
                | (SketchSizeKind::Ram { .. }, SketchSizeKind::Ram { .. })
        )
    }

    fn size_payload_ref(size_kind: &SketchSizeKind) -> &SketchSize {
        match size_kind {
            SketchSizeKind::Flash { size } | SketchSizeKind::Ram { size } => size,
        }
    }

    fn size_payload_mut(size_kind: &mut SketchSizeKind) -> &mut SketchSize {
        match size_kind {
            SketchSizeKind::Flash { size } | SketchSizeKind::Ram { size } => size,
        }
    }

    fn apply_size_delta(current: &mut SketchSizeKind, base: &SketchSizeKind) {
        let current_size = Self::size_payload_mut(current);
        let base_size = Self::size_payload_ref(base);

        current_size.previous = Some(SketchDeltaSize {
            absolute: Self::copy_size_value_i64(&base_size.current.absolute),
            relative: None,
        });

        let delta_abs = match (&current_size.current.absolute, &base_size.current.absolute) {
            (SizeValue::Known(curr), SizeValue::Known(prev)) => SizeValue::Known(curr - prev),
            _ => SizeValue::NotApplicable("N/A".to_string()),
        };

        current_size.delta = Some(SketchDeltaSize {
            absolute: delta_abs,
            relative: None,
        });
    }

    pub(super) fn get_sizes_summary_report(
        &self,
        sketch_reports: &[Sketch],
    ) -> Vec<SketchSizeKind> {
        let mut flash = SizeAggregateInternal::default();
        let mut ram = SizeAggregateInternal::default();

        for sketch in sketch_reports {
            for size_kind in &sketch.sizes {
                let size = Self::size_payload_ref(size_kind);
                let target = match size_kind {
                    SketchSizeKind::Flash { .. } => &mut flash,
                    SketchSizeKind::Ram { .. } => &mut ram,
                };

                if target.maximum.is_none() {
                    target.maximum = Self::copy_option_size_value_u64(&size.maximum);
                }

                if let SizeValue::Known(value) = size.current.absolute {
                    target.current_total += value;
                    target.has_current = true;
                }

                if let Some(previous) = &size.previous
                    && let SizeValue::Known(value) = previous.absolute
                {
                    target.previous_total += value;
                    target.has_previous = true;
                }
            }
        }

        let mut output = Vec::new();
        if flash.has_current || flash.has_previous {
            output.push(Self::aggregate_to_size_kind(true, flash));
        }
        if ram.has_current || ram.has_previous {
            output.push(Self::aggregate_to_size_kind(false, ram));
        }
        output
    }

    fn aggregate_to_size_kind(is_flash: bool, aggregate: SizeAggregateInternal) -> SketchSizeKind {
        let current = if aggregate.has_current {
            SizeValue::Known(aggregate.current_total)
        } else {
            SizeValue::NotApplicable("N/A".to_string())
        };
        let previous = if aggregate.has_previous {
            Some(SketchDeltaSize {
                absolute: SizeValue::Known(aggregate.previous_total),
                relative: None,
            })
        } else {
            None
        };
        let delta = if aggregate.has_current && aggregate.has_previous {
            Some(SketchDeltaSize {
                absolute: SizeValue::Known(aggregate.current_total - aggregate.previous_total),
                relative: None,
            })
        } else {
            None
        };

        let sketch_size = SketchSize {
            maximum: aggregate.maximum,
            current: SketchDeltaSize {
                absolute: current,
                relative: None,
            },
            previous,
            delta,
        };

        if is_flash {
            SketchSizeKind::Flash { size: sketch_size }
        } else {
            SketchSizeKind::Ram { size: sketch_size }
        }
    }

    fn copy_size_value_i64(value: &SizeValue<i64>) -> SizeValue<i64> {
        match value {
            SizeValue::Known(v) => SizeValue::Known(*v),
            SizeValue::NotApplicable(v) => SizeValue::NotApplicable(v.to_string()),
        }
    }

    fn copy_option_size_value_u64(value: &Option<SizeValue<u64>>) -> Option<SizeValue<u64>> {
        match value {
            Some(SizeValue::Known(v)) => Some(SizeValue::Known(*v)),
            Some(SizeValue::NotApplicable(v)) => Some(SizeValue::NotApplicable(v.to_string())),
            None => None,
        }
    }

    pub(super) fn get_board_sizes_from_summary(
        &self,
        sizes: &[SketchSizeKind],
    ) -> Option<Vec<BoardSize>> {
        let mut board_sizes = Vec::new();
        for size in sizes {
            match size {
                SketchSizeKind::Flash { size } => board_sizes.push(BoardSize::Flash {
                    maximum: Self::copy_option_size_value_u64(&size.maximum),
                }),
                SketchSizeKind::Ram { size } => board_sizes.push(BoardSize::Ram {
                    maximum: Self::copy_option_size_value_u64(&size.maximum),
                }),
            }
        }

        if board_sizes.is_empty() {
            None
        } else {
            Some(board_sizes)
        }
    }
}

#[derive(Default)]
struct SizeAggregateInternal {
    maximum: Option<SizeValue<u64>>,
    current_total: i64,
    previous_total: i64,
    has_current: bool,
    has_previous: bool,
}

#[cfg(test)]
mod tests {
    use arduino_report_size_deltas::report_structs::{
        AbsCount, Board, Report, SizeValue, Sketch, SketchDeltaSize, SketchSize, SketchSizeKind,
        SketchWarnings,
    };

    use crate::CompileSketches;

    #[test]
    fn get_sizes_from_output() {
        let cs = CompileSketches::default();
        let output = "Sketch uses 802 bytes (1%) of program storage space. Maximum is 1604 bytes.\nGlobal variables use 22 bytes (0%) of dynamic memory, leaving 6122 bytes for local variables. Maximum is 33 bytes.";
        let sizes = cs.get_sizes_from_output(output).unwrap();
        assert_eq!(sizes.len(), 2);
        // first entry should be Flash variant with absolute 802
        match &sizes[0] {
            SketchSizeKind::Flash { size } => {
                let flash_abs_val = match &size.current.absolute {
                    SizeValue::Known(n) => n.to_string(),
                    _ => "0".to_string(),
                };
                let digits: String = flash_abs_val
                    .chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect();
                let val = if digits.is_empty() {
                    0
                } else {
                    digits.parse::<i64>().unwrap()
                };
                assert_eq!(val, 802);
            }
            _ => panic!("expected flash size kind"),
        }
        // second entry should be Ram variant with absolute 22
        match &sizes[1] {
            SketchSizeKind::Ram { size } => {
                let ram_abs_val = match &size.current.absolute {
                    SizeValue::Known(n) => n.to_string(),
                    _ => "0".to_string(),
                };
                let digits: String = ram_abs_val.chars().filter(|c| c.is_ascii_digit()).collect();
                let val = if digits.is_empty() {
                    0
                } else {
                    digits.parse::<i64>().unwrap()
                };
                assert_eq!(val, 22);
            }
            _ => panic!("expected ram size kind"),
        }
    }

    #[test]
    fn get_warning_count_from_output() {
        let cs = CompileSketches::default();
        let out = "file.c:10:5: warning: something\nfile.c:20:2: warning: another\nno-warning-line";
        let cnt = cs.get_warning_count_from_output(out).unwrap();
        assert_eq!(cnt.current.absolute, 2);
    }

    #[test]
    fn get_sizes_summary_report() {
        let cs = CompileSketches::default();
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

        let res = cs.get_sizes_summary_report(&sketch_reports);
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
        let cs = CompileSketches::default();
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

        let base = Report {
            commit_hash: "base".into(),
            commit_url: "".into(),
            boards: vec![Board {
                board: cs.sketch_compiler.fqbn.clone(),
                sketches: vec![Sketch {
                    name: "S1".into(),
                    compilation_success: true,
                    sizes: vec![],
                    warnings: Some(SketchWarnings {
                        current: AbsCount { absolute: 2 },
                        previous: AbsCount::default(),
                        delta: AbsCount::default(),
                    }),
                }],
                sizes: None,
            }],
        };

        cs.apply_base_report(&mut sketches, Some(&base));
        let warnings = sketches[0].warnings.as_ref().unwrap();
        assert_eq!(warnings.previous.absolute, 2);
        assert_eq!(warnings.delta.absolute, 3);
    }
}
