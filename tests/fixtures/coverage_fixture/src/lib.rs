// LINE NUMBERS ARE LOAD-BEARING.
// Do not insert or remove lines above the function bodies without updating
// the constants in tests/coverage_integration_tests.rs and TC-006 in TST-0015.md.
//
// add_numbers body expression: line 8  (COVERED_LINE_START)
// never_called body expression: line 13 (UNCOVERED_LINE_START)

pub fn add_numbers(a: i32, b: i32) -> i32 {
    a + b
}

pub fn never_called(x: i32) -> i32 {
    x * 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(add_numbers(2, 3), 5);
    }
}
