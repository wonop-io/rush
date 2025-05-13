#[cfg(test)]
mod simple_tests {
    #[test]
    fn test_simple_addition() {
        assert_eq!(2 + 2, 4);
    }
    
    #[test]
    fn test_simple_multiplication() {
        assert_eq!(2 * 3, 6);
    }
    
    #[test]
    fn test_simple_subtraction() {
        assert_eq!(5 - 3, 2);
    }
}