use lazy_static::lazy_static;
use std::collections::HashMap;
use tera::Tera;

use serde_json::value::{to_value, Value};
use std::error::Error;
use tera::{Context, Result};
lazy_static! {
    pub static ref TEMPLATES: Tera = {
        let template_path = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), "src/builder/templates/**");

        // tera.autoescape_on(vec!["html", ".sql"]);
        //
        let mut tera = match Tera::new(&template_path) {
            Ok(t) => t,
            Err(e) => {
                println!("Parsing error(s): {}", e);
                ::std::process::exit(1);
            }
        };

        tera.register_filter("uppercase", uppercase_filter);
        tera.register_filter("lowercase", lowercase_filter);

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
