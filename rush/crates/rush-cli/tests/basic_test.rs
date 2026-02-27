#[cfg(test)]
mod basic_tests {
    #[test]
    fn test_basic_addition() {
        // A simple test to verify the testing infrastructure works
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn test_basic_strings() {
        let s1 = "hello";
        let s2 = "world";
        assert_eq!(format!("{s1} {s2}"), "hello world");
    }

    #[test]
    fn test_basic_vectors() {
        let v = [1, 2, 3];

        assert_eq!(v.len(), 3);
        assert_eq!(v[0], 1);
        assert_eq!(v[1], 2);
        assert_eq!(v[2], 3);
    }
}
