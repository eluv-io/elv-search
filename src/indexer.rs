use elvwasm::{implement_bitcode_module, BitcodeContext, ErrorKinds};
use serde::Deserialize;
use serde_json::{json, Result, Value};
use std::collections::HashMap;
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
    fn parse_index_config(config_value: Value) -> Result<IndexerConfig> {
        // Read config string as serde_json Value

        // Parse config into IndexerConfig
        let indexer_config_val: &Value = &config_value["indexer"]["config"]["indexer"];
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

#[derive(Deserialize)]
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
struct PathToFieldsGraph {
    children: HashMap<String, PathToFieldsGraph>,
    fields: Vec<String>,
    path: String,
}

impl PathToFieldsGraph {
    fn new(path: &str) -> PathToFieldsGraph {
        PathToFieldsGraph {
            children: HashMap::new(),
            fields: Vec::new(),
            path: "".to_string(),
        }
    }

    fn new_from_fields(fields: &Vec<FieldConfig>) -> PathToFieldsGraph {
        let mut path_to_fields_graph = PathToFieldsGraph::new("");
        for field_config in fields {
            for path in &field_config.paths {
                path_to_fields_graph.add_field_to_path(&field_config.name, path);
            }
        }
        return path_to_fields_graph;
    }

    /**
     * Adds field_name to be indexed at given path, given as string with delimiter '.'.
     */
    fn add_field_to_path(&mut self, field_name: &str, path: &str) -> () {
        let mut path_components = path.split(".").peekable();
        let mut curr_node: &mut PathToFieldsGraph = self;
        let mut curr_path = String::new();
        while let Some(path_component) = path_components.next() {
            // If we're at the correct node, push path to this node.
            if path_components.peek().is_none() || path_component.is_empty() {
                curr_node.fields.push(field_name.to_string());
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
                    PathToFieldsGraph::new(&curr_path),
                );
            }
            curr_node = curr_node.children.get_mut(path_component).unwrap(); // Recurse down tree
        }
    }

    fn map(&self, f: impl Fn(&Vec<String>, &str)) -> () {
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

fn extract_body(v: Value) -> Option<Value> {
    let obj = match v.as_object() {
        Some(v) => v,
        None => return None,
    };
    let mut full_result = true;
    let res = match obj.get("result") {
        Some(m) => m,
        None => match obj.get("http") {
            Some(h) => {
                full_result = false;
                h
            }
            None => return None,
        },
    };
    if full_result {
        let http = match res.get("http") {
            Some(h) => h,
            None => return None,
        };
        return match http.get("body") {
            Some(b) => Some(b.clone()),
            None => None,
        };
    }
    return match res.get("body") {
        Some(b) => Some(b.clone()),
        None => None,
    };
}

pub fn create_index(bcc: &mut BitcodeContext) -> CallResult {
    // Read request
    let http_p = &bcc.request.params.http;
    let query_params = &http_p.query;
    BitcodeContext::log(&format!(
        "In create_index hash={} headers={:#?} query params={:#?}",
        &bcc.request.q_info.hash, &http_p.headers, query_params
    ));
    let id = &bcc.request.id;

    // FIXME Get index configuration
    let config_value: Value = serde_json::from_str(&http_p.query["index_config"][0])?;
    let indexer_config: IndexerConfig = IndexerConfig::parse_index_config(config_value)?;

    // Construct path to fields graph
    let path_to_fields_graph = PathToFieldsGraph::new_from_fields(&indexer_config.fields);

    // Create index in directory
    let mut input_data = json!({
        "directory": "index" //TODO is this correct directory?
    });
    BitcodeContext::log(&format!("before BUILDER"));
    bcc.new_index_builder(input_data)?;
    BitcodeContext::log(&format!("NEW INDEX BUILDER"));

    // Add fields to schema builder
    for field_config in indexer_config.fields {
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
                        return bcc.make_error_with_kind(ErrorKinds::BadHttpParams(
                            "could not find key document-create-id",
                        ))
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
                        return bcc.make_error_with_kind(ErrorKinds::BadHttpParams(
                            "could not find key document-create-id",
                        ))
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
    // v = json!({});
    // bcc.builder_build(v.clone())?;
    // let doc_old_man_u8 = bcc.document_create(v)?;
    // BitcodeContext::log(&format!("DOC CREATE"));
    // let doc_old_man:serde_json::Value = serde_json::from_slice(&doc_old_man_u8)?;
    // console_log(&format!("obj_old = {:?}", &doc_old_man));
    // let doc_id = match extract_body(doc_old_man.clone()){
    //     Some(o) => o.get("document-create-id").unwrap().as_u64(),
    //     None => return bcc.make_error_with_kind(ErrorKinds::BadHttpParams("could not find key document-create-id")),
    // };
    // v = json!({ "field": field_title, "value": "The Old Man and the Sea", "doc_id": doc_id});
    // bcc.document_add_text(v)?;
    // BitcodeContext::log(&format!("DOC ADD TEXT TITLE"));
    // v = json!({ "field": field_body, "value": S_OLD_MAN, "doc_id": doc_id});
    // bcc.document_add_text(v)?;
    // BitcodeContext::log(&format!("DOC ADD TEXT BODY"));
    // v = json!({});
    // bcc.document_create_index(v.clone())?;
    // bcc.index_create_writer(v)?;
    // v = json!({ "document_id": doc_id});
    // bcc.index_add_document(v)?;
    // v = json!({});
    // bcc.index_writer_commit(v)?;
    // let part_u8 = bcc.archive_index_to_part()?;
    // let part_hash:serde_json::Value = serde_json::from_slice(&part_u8)?;
    // let b = extract_body(part_hash.clone());
    // let body_hash = b.unwrap_or(json!({}));
    // BitcodeContext::log(&format!("part hash = {}, bosy = {}", &part_hash.to_string(), &body_hash.to_string()));
    // bcc.make_success_json(&json!(
    //     {
    //         "headers" : "application/json",
    //         "body" : "SUCCESS",
    //         "result" : body_hash,
    //     }), id)
    Ok(Vec::new())
}

impl Indexer {
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
        let config_value: Value = serde_json::from_str(INDEX_CONFIG)
            .expect("Could not read index config into json value.");
        let indexer_config: IndexerConfig = IndexerConfig::parse_index_config(config_value)
            .expect("Could not parse indexer config.");
    }

    #[test]
    fn test_path_to_fields_graoh() -> () {
        let config_value: Value = serde_json::from_str(INDEX_CONFIG)
            .expect("Could not read index config into json value.");
        let indexer_config: IndexerConfig = IndexerConfig::parse_index_config(config_value)
            .expect("Could not parse indexer config.");
        let path_to_fields_graph = PathToFieldsGraph::new_from_fields(&indexer_config.fields);
    }
}
