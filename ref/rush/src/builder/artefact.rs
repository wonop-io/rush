use crate::builder::BuildContext;
use tera::{Context, Tera};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Artefact {
    pub input_path: String,
    pub output_path: String,
    pub template: String,
}

impl Artefact {
    pub fn new(input_path: String, output_path: String) -> Self {
        let template = std::fs::read_to_string(&input_path)
            .unwrap_or_else(|_| panic!("Failed to read template from input file {}", input_path));
        Artefact {
            input_path,
            output_path,
            template,
        }
    }

    pub fn render(&self, context: &BuildContext) -> String {
        let template = self.template.clone();

        let mut tera = Tera::default();
        tera.add_raw_templates(vec![(&self.input_path, template)])
            .unwrap();
        let context = Context::from_serialize(context).expect("Could not create context");

        match tera.render(&self.input_path, &context) {
            Ok(rendered) => rendered,
            Err(e) => {
                println!("File: {}", self.input_path);
                println!("Context: {:#?}", context);
                panic!("Failed to render template: {}", e);
            }
        }
        // .expect("Could not render template")
    }

    pub fn render_to_file(&self, context: &BuildContext) {
        let rendered = self.render(context);
        std::fs::write(&self.output_path, rendered).expect("Failed to write to output file");
    }
}
