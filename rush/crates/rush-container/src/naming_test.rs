//! Integration test for container naming consistency

#[cfg(test)]
mod tests {
    use rush_core::naming::NamingConvention;

    #[test]
    fn test_container_naming_consistency() {
        // Test case that was failing before
        let product_name = "compoundcoders.com";
        let component_name = "frontend";

        // All these should produce the same result
        let name1 = NamingConvention::container_name(product_name, component_name);
        let name2 = format!("{}-{}", product_name, component_name);

        assert_eq!(name1, name2);
        assert_eq!(name1, "compoundcoders.com-frontend");

        // Ensure no underscores are used
        assert!(!name1.contains('_'), "Container name should not contain underscores");
    }

    #[test]
    fn test_all_naming_methods_consistent() {
        let product = "myproduct";
        let component = "backend";

        let container_name = NamingConvention::container_name(product, component);
        let image_name = NamingConvention::image_name(product, component);

        // Container and image names should be the same for local usage
        assert_eq!(container_name, image_name);
        assert_eq!(container_name, "myproduct-backend");
    }
}