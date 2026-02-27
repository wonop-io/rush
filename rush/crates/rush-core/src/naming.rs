//! Centralized naming conventions for Docker containers and images
//!
//! This module provides consistent naming functions to ensure that container
//! and image names are generated uniformly across the entire Rush codebase.

use std::fmt;

/// Provides centralized naming conventions for Rush components
#[derive(Debug, Clone)]
pub struct NamingConvention;

impl NamingConvention {
    /// Generate a container name from product and component names
    ///
    /// # Arguments
    /// * `product_name` - The product name (e.g., "compoundcoders.com")
    /// * `component_name` - The component name (e.g., "frontend")
    ///
    /// # Returns
    /// A formatted container name using hyphen separation (e.g., "compoundcoders.com-frontend")
    ///
    /// # Example
    /// ```
    /// use rush_core::naming::NamingConvention;
    ///
    /// let name = NamingConvention::container_name("myproduct", "backend");
    /// assert_eq!(name, "myproduct-backend");
    /// ```
    pub fn container_name(product_name: &str, component_name: &str) -> String {
        format!("{product_name}-{component_name}")
    }

    /// Generate an image name from product and component names
    ///
    /// # Arguments
    /// * `product_name` - The product name
    /// * `component_name` - The component name
    ///
    /// # Returns
    /// A formatted image name using hyphen separation
    pub fn image_name(product_name: &str, component_name: &str) -> String {
        format!("{product_name}-{component_name}")
    }

    /// Generate a fully qualified image name with registry
    ///
    /// # Arguments
    /// * `registry` - Optional Docker registry URL
    /// * `namespace` - Optional namespace within the registry
    /// * `product_name` - The product name
    /// * `component_name` - The component name
    ///
    /// # Returns
    /// A fully qualified image name
    pub fn full_image_name(
        registry: Option<&str>,
        namespace: Option<&str>,
        product_name: &str,
        component_name: &str,
    ) -> String {
        let base_name = Self::image_name(product_name, component_name);

        match (registry, namespace) {
            (Some(reg), Some(ns)) => format!("{reg}/{ns}/{base_name}"),
            (Some(reg), None) => format!("{reg}/{base_name}"),
            (None, Some(ns)) => format!("{ns}/{base_name}"),
            (None, None) => base_name,
        }
    }

    /// Validate that a name is suitable for use as a Docker container/image name
    ///
    /// Docker naming rules:
    /// - Must start with a letter or number
    /// - Can contain lowercase letters, numbers, hyphens, underscores, and periods
    /// - Cannot have consecutive periods, hyphens, or underscores
    ///
    /// # Arguments
    /// * `name` - The name to validate
    ///
    /// # Returns
    /// `Ok(())` if valid, `Err(reason)` if invalid
    pub fn validate_name(name: &str) -> Result<(), String> {
        if name.is_empty() {
            return Err("Name cannot be empty".to_string());
        }

        // Check first character
        let first_char = name.chars().next().unwrap();
        if !first_char.is_ascii_alphanumeric() {
            return Err(format!(
                "Name must start with a letter or number, found '{first_char}'"
            ));
        }

        // Check for valid characters
        let mut prev_char = ' ';
        for c in name.chars() {
            if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' && c != '_' && c != '.' {
                // Allow uppercase for now, as some products use it
                if !c.is_ascii_uppercase() {
                    return Err(format!("Invalid character '{c}' in name"));
                }
            }

            // Check for consecutive special characters
            if (c == '.' || c == '-' || c == '_')
                && (prev_char == '.' || prev_char == '-' || prev_char == '_')
            {
                return Err(format!(
                    "Cannot have consecutive special characters '{prev_char}{c}' in name"
                ));
            }
            prev_char = c;
        }

        Ok(())
    }

    /// Sanitize a name to make it suitable for use as a Docker container/image name
    ///
    /// # Arguments
    /// * `name` - The name to sanitize
    ///
    /// # Returns
    /// A sanitized version of the name
    pub fn sanitize_name(name: &str) -> String {
        let mut result = String::new();
        let mut prev_special = false;

        for c in name.chars() {
            if c.is_ascii_alphanumeric() {
                result.push(c.to_ascii_lowercase());
                prev_special = false;
            } else if (c == '-' || c == '.' || c == '_') && !prev_special && !result.is_empty() {
                result.push(c);
                prev_special = true;
            }
            // Skip invalid characters
        }

        // Remove trailing special characters
        while result.ends_with('.') || result.ends_with('-') || result.ends_with('_') {
            result.pop();
        }

        result
    }
}

impl fmt::Display for NamingConvention {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NamingConvention(hyphen-separated)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_name() {
        assert_eq!(
            NamingConvention::container_name("myproduct", "frontend"),
            "myproduct-frontend"
        );
        assert_eq!(
            NamingConvention::container_name("compoundcoders.com", "backend"),
            "compoundcoders.com-backend"
        );
    }

    #[test]
    fn test_image_name() {
        assert_eq!(
            NamingConvention::image_name("myproduct", "frontend"),
            "myproduct-frontend"
        );
    }

    #[test]
    fn test_full_image_name() {
        assert_eq!(
            NamingConvention::full_image_name(None, None, "product", "component"),
            "product-component"
        );
        assert_eq!(
            NamingConvention::full_image_name(Some("registry.io"), None, "product", "component"),
            "registry.io/product-component"
        );
        assert_eq!(
            NamingConvention::full_image_name(
                Some("registry.io"),
                Some("namespace"),
                "product",
                "component"
            ),
            "registry.io/namespace/product-component"
        );
        assert_eq!(
            NamingConvention::full_image_name(None, Some("namespace"), "product", "component"),
            "namespace/product-component"
        );
    }

    #[test]
    fn test_validate_name() {
        assert!(NamingConvention::validate_name("valid-name").is_ok());
        assert!(NamingConvention::validate_name("valid.name").is_ok());
        assert!(NamingConvention::validate_name("valid_name").is_ok());
        assert!(NamingConvention::validate_name("123valid").is_ok());

        assert!(NamingConvention::validate_name("").is_err());
        assert!(NamingConvention::validate_name("-invalid").is_err());
        assert!(NamingConvention::validate_name("invalid..name").is_err());
        assert!(NamingConvention::validate_name("invalid--name").is_err());
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(NamingConvention::sanitize_name("Valid-Name"), "valid-name");
        assert_eq!(
            NamingConvention::sanitize_name("invalid--name"),
            "invalid-name"
        );
        assert_eq!(NamingConvention::sanitize_name("---invalid"), "invalid");
        assert_eq!(NamingConvention::sanitize_name("name!!!"), "name");
        assert_eq!(NamingConvention::sanitize_name("name..."), "name");
    }
}
