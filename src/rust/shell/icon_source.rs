//! Permanent shortcut icon order: explicit file, program container icon0,
//! generic program icon, then the painter's built-in fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source<'a> {
    External(&'a [u8]),
    Container,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    External,
    Container,
    Standard,
}
pub struct Plan {
    stages: [Stage; 3],
    len: usize,
}
impl Plan {
    pub fn stages(&self) -> &[Stage] {
        &self.stages[..self.len]
    }
}
pub fn classify(value: &[u8]) -> Source<'_> {
    if value.is_empty() || value.eq_ignore_ascii_case(b"INTERNAL") {
        Source::Container
    } else {
        Source::External(value)
    }
}
pub fn plan(source: Source<'_>, program: bool) -> Plan {
    let mut result = Plan {
        stages: [Stage::Standard; 3],
        len: 0,
    };
    if matches!(source, Source::External(_)) {
        result.stages[result.len] = Stage::External;
        result.len += 1;
    }
    if program {
        result.stages[result.len] = Stage::Container;
        result.stages[result.len + 1] = Stage::Standard;
        result.len += 2;
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_empty_internal_case_variants_mean_container() {
        for s in [b"".as_slice(), b"INTERNAL", b"internal", b"Internal"] {
            assert_eq!(classify(s), Source::Container);
        }
    }
    #[test]
    fn original_explicit_path_remains_permanent_external_branch() {
        let p = b"C:\\R4OS\\Media\\Icons\\Terminal.ico";
        assert_eq!(classify(p), Source::External(p));
    }
    #[test]
    fn original_explicit_program_path_container_standard_order() {
        assert_eq!(
            plan(classify(b"C:\\X.ICO"), true).stages(),
            &[Stage::External, Stage::Container, Stage::Standard]
        );
    }
    #[test]
    fn original_internal_program_skips_external() {
        assert_eq!(
            plan(classify(b"INTERNAL"), true).stages(),
            &[Stage::Container, Stage::Standard]
        );
    }
    #[test]
    fn original_nonprogram_path_never_container_or_standard() {
        assert_eq!(
            plan(classify(b"C:\\X.ICO"), false).stages(),
            &[Stage::External]
        );
    }
    #[test]
    fn original_nonprogram_without_icon_uses_builtin() {
        assert!(plan(classify(b""), false).stages().is_empty());
    }
}
