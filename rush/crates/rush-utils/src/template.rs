use lazy_static::lazy_static;
use std::collections::HashMap;
use tera::Tera;

use serde_json::value::{to_value, Value};
use tera::{try_get_value, Context, Result};

lazy_static! {
    pub static ref TEMPLATES: Tera = {
        let template_path = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), "templates/**/*");

        let mut tera = match Tera::new(&template_path) {
            Ok(t) => t,
            Err(e) => {
                println!("Parsing error(s): {e}");
                ::std::process::exit(1);
            }
        };

        tera.register_filter("uppercase", uppercase_filter);
        tera.register_filter("lowercase", lowercase_filter);
        tera.register_filter("envname", to_env_name_filter);
        tera
    };
}

pub fn uppercase_filter(value: &Value, _: &HashMap<String, Value>) -> Result<Value> {
    let s = try_get_value!("uppercase_filter", "value", String, value);
    Ok(to_value(s.to_uppercase()).unwrap())
}

pub fn lowercase_filter(value: &Value, _: &HashMap<String, Value>) -> Result<Value> {
    let s = try_get_value!("lowercase_filter", "value", String, value);
    Ok(to_value(s.to_lowercase()).unwrap())
}

pub fn to_env_name_filter(value: &Value, _: &HashMap<String, Value>) -> Result<Value> {
    let s = try_get_value!("to_env_name_filter", "value", String, value);
    let transformed = s.to_uppercase().replace("-", "_");
    Ok(to_value(transformed).unwrap())
}

/// Renders a template with the given context
pub fn render_template(template_name: &str, context: &Context) -> Result<String> {
    TEMPLATES.render(template_name, context)
}

/// Loads a template string and renders it with the given context
pub fn render_template_string(template_string: &str, context: &Context) -> Result<String> {
    Tera::one_off(template_string, context, true)
}
