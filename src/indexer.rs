use crate::utils::extract_body;

use elvwasm::{implement_bitcode_module, BitcodeContext, ErrorKinds};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::error::Error;
use std::ptr::NonNull;
use std::str::Split;

use wapc;
use wapc_guest::CallResult;

#[derive(Deserialize)]
pub struct RootConfig {
    pub(crate) content: String,
    pub(crate) library: String,
}

#[derive(Deserialize)]
pub struct FabricConfig {
    pub(crate) policy: Value,
    pub(crate) root: RootConfig,
}

#[derive(Deserialize)]
pub struct IndexerConfig {
    #[serde(rename = "type")]
    pub(crate) indexer_type: String,
    pub(crate) document: Value,
    pub(crate) fields: Vec<FieldConfig>,
}

impl IndexerConfig {
    /**
     * Given a string representing the JSON index config, returns
     * and IndexerConfig value with proper fields filled out.
     */
    pub fn parse_index_config(
        config_value: &Value,
    ) -> Result<IndexerConfig, Box<dyn Error + Send + Sync>> {
        // Read config string as serde_json Value

        // Parse config into IndexerConfig
        let indexer_config_val: &Value = &config_value["indexer"];
        let indexer_arguments_val = &indexer_config_val["arguments"];
        let mut field_configs: Vec<FieldConfig> = Vec::new();
        for (field_name, field_value) in indexer_arguments_val["fields"].as_object().unwrap() {
            field_configs.push(FieldConfig {
                name: field_name.to_string(),
                options: field_value["options"].clone(),
                field_type: serde_json::from_value(field_value["type"].clone())?,
                paths: serde_json::from_value(field_value["paths"].clone())?,
            });
        }
        Ok(IndexerConfig {
            indexer_type: serde_json::from_value(indexer_config_val["type"].clone())?,
            document: indexer_arguments_val["document"].clone(),
            fields: field_configs,
        })
    }
}

#[derive(Deserialize, Debug)]
pub struct FieldConfig {
    pub(crate) name: String,
    #[serde(rename = "type")]
    pub(crate) field_type: String,
    options: Value,
    pub(crate) paths: Vec<String>,
}

/**
 * Represents structure of paths with each node as a component. Every node
 * stores the field names to index at that point. Root path is an empty String.
 */
#[derive(Debug)]
pub struct PathToFieldsNode<'a> {
    children: HashMap<String, PathToFieldsNode<'a>>,
    fields: Vec<&'a FieldConfig>,
    path: String,
}

impl<'a> PathToFieldsNode<'a> {
    pub fn new(path: String) -> PathToFieldsNode<'a> {
        PathToFieldsNode {
            children: HashMap::new(),
            fields: Vec::new(),
            path: path,
        }
    }

    pub fn new_from_fields(fields: &'a Vec<FieldConfig>) -> PathToFieldsNode<'a> {
        let mut path_to_fields_graph = PathToFieldsNode::new("".to_string());
        for field_config in fields {
            for path in &field_config.paths {
                path_to_fields_graph.add_field_to_path(field_config, path);
            }
        }
        return path_to_fields_graph;
    }

    /**
     * Adds field_name to be indexed at given path, given as string with delimiter '.'.
     */
    fn add_field_to_path(&mut self, field_config: &'a FieldConfig, path: &str) -> () {
        let mut path_components = path.split(".").peekable();
        let mut curr_node: &mut PathToFieldsNode = self;
        let mut curr_path = String::new();
        while let Some(path_component) = path_components.next() {
            // If we're at the correct node, push field_config to this node.
            if path_components.peek().is_none() || path_component.is_empty() {
                curr_node.fields.push(field_config);
                return;
            }
            // Update current path in tree
            if !curr_path.is_empty() {
                curr_path.push('.');
            }
            curr_path.push_str(path_component);
            // Recurse down tree to get to correct path.
            if !curr_node.children.contains_key(path_component) {
                curr_node.children.insert(
                    path_component.to_string(),
                    PathToFieldsNode::new(curr_path.clone()),
                );
            }
            curr_node = curr_node.children.get_mut(path_component).unwrap(); // Recurse down tree
        }
    }

    /**
     * Returns list of names of children nodes.
     */
    pub fn get_children_iter(&self) -> std::collections::hash_map::Iter<String, PathToFieldsNode> {
        self.children.iter()
    }

    pub fn get_fields(&self) -> &Vec<&FieldConfig> {
        &self.fields
    }

    fn map(&self, f: impl Fn(&Vec<&FieldConfig>, &str)) -> () {
        f(&self.fields, &self.path);
        for (_key, child) in &self.children {
            child.map(&f);
        }
    }
}
pub struct Indexer {
    // index: Option<Index>,
    // index_path: String,
    // index_writer: Option<IndexWriter>,
    // path_to_field_names: Map<String, String>,
    // schema_builder: Option<SchemaBuilder>,
    // schema: Option<Schema>,
}

impl Indexer {
    /**
     * Builds an index with fields as specified in indexer_config
     */
    pub fn create_index(
        bcc: &BitcodeContext,
        indexer_config: &IndexerConfig,
    ) -> Result<Indexer, Box<dyn Error + Send + Sync>> {
        // Read request
        let http_p = &bcc.request.params.http;
        let query_params = &http_p.query;
        BitcodeContext::log(&format!(
            "In create_index hash={} headers={:#?} query params={:#?}",
            &bcc.request.q_info.hash, &http_p.headers, query_params
        ));
        let id = &bcc.request.id;

        // Construct path to fields graph
        let path_to_fields_graph = PathToFieldsNode::new_from_fields(&indexer_config.fields);

        // Create index in directory
        let mut input_data = json!({
            "directory": "index" //TODO is this correct directory?
        });
        BitcodeContext::log(&format!("before BUILDER"));
        bcc.new_index_builder(input_data)?;
        BitcodeContext::log(&format!("NEW INDEX BUILDER"));

        // Add fields to schema builder
        for field_config in &indexer_config.fields {
            match field_config.field_type.as_str() {
                "text" => {
                    input_data = json!({
                        "name": field_config.name,
                        "type": 1 as u8, //FIXME this should be a TextOption
                        "stored": true,
                    });
                    let field_title_vec = bcc.builder_add_text_field(input_data)?;
                    let ft_json: serde_json::Value = serde_json::from_slice(&field_title_vec)?;
                    match extract_body(ft_json.clone()) {
                        Some(o) => o.get("field").unwrap().as_u64(),
                        None => {
                            return Err(Box::new(ErrorKinds::BadHttpParams(
                                "could not find key document-create-id",
                            )))
                        }
                    };
                    BitcodeContext::log(&format!("ADDED TEXT FIELD."));
                }
                "string" => {
                    input_data = json!({
                        "name": field_config.name,
                        "type": 1 as u8, //FIXME this should be a TextOption. What is the right number here?
                        "stored": true,
                    });
                    let field_title_vec = bcc.builder_add_text_field(input_data)?;
                    let ft_json: serde_json::Value = serde_json::from_slice(&field_title_vec)?;
                    match extract_body(ft_json.clone()) {
                        Some(o) => o.get("field").unwrap().as_u64(),
                        None => {
                            return Err(Box::new(ErrorKinds::BadHttpParams(
                                "could not find key document-create-id",
                            )))
                        }
                    };
                    BitcodeContext::log(&format!("ADDED STRING FIELD."));
                }
                _ => panic!("unknown field type"),
            }
        }

        // Build index
        input_data = json!({});
        bcc.builder_build(input_data)?;

        Ok(Indexer {})
    }
    // fn new(index_path: String) -> Indexer {
    //     Indexer {
    //         index: None,
    //         index_path: index_path,
    //         index_writer: None,
    //         fields: Vec::new(),
    //         schema_builder: Some(Schema::builder()),
    //         schema: None,
    //     }
    // }

    // fn add_field(&mut self, field_config: FieldConfig) -> Result<(), ()> {
    //     // Get schema builder or create new one
    //     // let schema_builder = self.get_schema_builder();
    //     let schema_builder = match &mut self.schema_builder {
    //         Some(x) => x,
    //         None => {
    //             self.schema_builder = Some(Schema::builder());
    //             self.schema_builder.as_mut().unwrap()
    //         }
    //     };
    //     // Add field to schema
    //     match field_config.field_type {
    //         "text" => schema_builder.add_text_field(&field_config.name, TEXT | STORED),
    //         _ => panic!("unknown field type"),
    //     };

    //     self.fields.push(field_config.name);

    //     Ok(())
    // }

    // fn build_schema(&mut self) -> Result<(), ()> {
    //     let schema_builder = match self.schema_builder.take() {
    //         Some(x) => x,
    //         None => panic!("No schema builder initialized."),
    //     };
    //     self.schema = Some(schema_builder.build());
    //     Ok(())
    // }

    // fn build_index_writer(&mut self) -> Result<(), ()> {
    //     let index = Index::builder()
    //         .schema(self.schema.as_ref().unwrap().clone())
    //         .create_in_dir(&self.index_path)
    //         .expect("Failed to build index.");
    //     self.index_writer = Some(index.writer(150_000_000).expect("Failed to create writer"));
    //     Ok(())
    // }
}

// #[derive(Error, Debug, Clone)]
// pub enum ErrorKinds {
//   #[error("Other Error : `{0}`")]
//   Other(String),
//   #[error("Not Recognized : `{0}`")]
//   UnRecognizedCommand(String),
//   #[error("Permission : `{0}`")]
//   Permission(String),
//   #[error("IO : `{0}`")]
//   IO(String),
//   #[error("Exist : `{0}`")]
//   Utf8Error(std::str::Utf8Error),
//   #[error("NotExist : `{0}`")]
//   NotExist(String),
//   #[error("IsDir : `{0}`")]
//   IsDir(String),
//   #[error("NotDir : `{0}`")]
//   NotDir(String),
//   #[error("Finalized : `{0}`")]
//   BadInitialization(String),
//   #[error("NotFinalized : `{0}`")]
//   NotFinalized(String),
//   #[error("BadParams : `{0}`")]
//   BadParams(String),
//   #[error("Search : `{0}`")]
//   Search(String),
// }

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;
    use test_utils::test_metadata::INDEX_CONFIG;

    #[test]
    fn test_parse_index_config() -> () {
        let index_object_meta: Value = serde_json::from_str(INDEX_CONFIG)
            .expect("Could not read index object into json value.");
        let config_value: &Value = &index_object_meta["indexer"]["config"];
        let indexer_config: IndexerConfig = IndexerConfig::parse_index_config(config_value)
            .expect("Could not parse indexer config.");

        /* Assert that indexer_config fields are correctly filled out. */
        assert_eq!(22, indexer_config.fields.len());
        assert_eq!("metadata-text", indexer_config.indexer_type);
        assert!(config_value["indexer"]["arguments"]["document"] == indexer_config.document);
    }

    #[test]
    fn test_path_to_fields_graph() -> () {
        let index_object_meta: Value = serde_json::from_str(INDEX_CONFIG)
            .expect("Could not read index object into json value.");
        let config_value: &Value = &index_object_meta["indexer"]["config"];
        let indexer_config: IndexerConfig = IndexerConfig::parse_index_config(config_value)
            .expect("Could not parse indexer config.");

        let path_to_fields_root = PathToFieldsNode::new_from_fields(&indexer_config.fields);
        /* The tree should store 23 FieldConfigs, since one FieldConfig has two paths. */
        let mut queue: Vec<&PathToFieldsNode> = Vec::new();
        queue.push(&path_to_fields_root);
        let mut field_configs_found = 0;
        while !queue.is_empty() {
            let path_to_fields_node = queue.pop().unwrap();
            field_configs_found += path_to_fields_node.fields.len();
            for child in path_to_fields_node.children.values() {
                queue.push(child);
            }
        }
        assert_eq!(23, field_configs_found);
    }
}
