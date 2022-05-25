use crate::indexer::{self, FabricConfig, Indexer, IndexerConfig, PathToFieldsNode};
use crate::utils::extract_body;

use std::error::Error;

use elvwasm::{implement_bitcode_module, BitcodeContext, ErrorKinds};
use serde_json::{json, Value};
use wapc_guest::CallResult;

// fn request_metadata(bcc: &mut BitcodeContext) -> CallResult {
//     bcc.sqmd_get_json(path)
// }

// fn resolve_link(bcc: &mut BitcodeContext, link: &str) -> Result<Value, ErrorKinds> {}
// fn get_root_metadata(bcc: &mut BitcodeContext, qlib_id: &str, qhash: &str) -> CallResult {
//     let root_metadata = bcc.sqmd_get_json_external(qlib_id, qhash, "")?;
//     let root_metadata_val: Value = serde_json::from_slice(&root_metadata)?;
//     return Ok(root_metadata_val);
// }

struct FabricLink {
    link: String,
}

impl FabricLink {
    fn new(link: &str, parent_hash: &str) -> FabricLink {
        if link.starts_with("./") {
            return FabricLink {
                link: format!("/qfab/{}/{}", parent_hash, &link[2..]),
            };
        } else {
            return FabricLink {
                link: link.to_string(),
            };
        }
    }
}
/**
 * Perform crawl on index object.
 * TODO: be able to use an existing index
 */
fn run(bcc: &BitcodeContext) -> CallResult {
    // Read index config
    let indexer_config_vec = bcc.sqmd_get_json("indexer/config")?;
    let indexer_config_val = &(serde_json::from_slice(&indexer_config_vec)?);
    let indexer_config = IndexerConfig::parse_index_config(indexer_config_val)?;
    // Read fabric config
    let fabric_config: FabricConfig = serde_json::from_value(indexer_config_val["fabric"].clone())?;

    // Instantiate crawler
    let mut crawler = Crawler {
        bcc: bcc,
        indexer_config: indexer_config,
        fabric_config: fabric_config,
        path_to_fields_root: None,
        crawl_queue: Vec::new(),
    };

    crawler.crawl()
}

struct Crawler<'a, 'b> {
    bcc: &'a BitcodeContext<'b>,
    indexer_config: IndexerConfig,
    fabric_config: FabricConfig,
    path_to_fields_root: Option<Box<PathToFieldsNode<'a>>>,
    crawl_queue: Vec<Value>, // Stores metadata values that represent entire values to crawl (will switch to objectIds once we turn off recursive resolution)
}

impl<'a, 'b> Crawler<'a, 'b> {
    fn crawl(&'a mut self) -> CallResult {
        // Create and build index with specified fields.
        Indexer::create_index(self.bcc, &self.indexer_config)?;

        // Get root content info
        let root_qlib_id = &self.fabric_config.root.library;
        let qVersionsVec: Vec<u8> = self
            .bcc
            .q_get_versions(&self.fabric_config.root.content, false)?;
        let qVersions: elvwasm::QRef =
            serde_json::from_str(std::str::from_utf8(&qVersionsVec).unwrap()).unwrap();
        let root_qhash = &qVersions.versions.get(0).unwrap().hash; //FIXME are the versions ordered?

        // Initialize path to fields graph to be used in crawl.
        self.path_to_fields_root = Some(Box::new(PathToFieldsNode::new_from_fields(
            &self.indexer_config.fields,
        )));

        // Create index writer
        let mut input = json!({});
        self.bcc.index_create_writer(input)?;

        // Crawl content and add documents to index.
        self.crawl_qhash(
            root_qlib_id,
            root_qhash,
            &self.path_to_fields_root.as_ref().unwrap(),
        )?;

        input = json!({});
        self.bcc.index_writer_commit(input)?;

        let part_u8 = self.bcc.archive_index_to_part()?;

        let part_hash: serde_json::Value = serde_json::from_slice(&part_u8)?;
        let b = extract_body(part_hash.clone());
        let body_hash = b.unwrap_or(json!({}));
        BitcodeContext::log(&format!(
            "part hash = {}, bosy = {}",
            &part_hash.to_string(),
            &body_hash.to_string()
        ));

        let id = &self.bcc.request.id;
        self.bcc.make_success_json(
            &json!(
            {
                "headers" : "application/json",
                "body" : "SUCCESS",
                "result" : body_hash,
            }),
            id,
        );

        Ok(Vec::new())
    }

    /**
     * Crawl object given its qhash. If any of its fields are specified in the indexer config,
     * this creates a document with the appropriate fields and adds it to the index.
     */
    fn crawl_qhash(
        &self,
        qlib_id: &str,
        qhash: &str,
        path_to_fields_node: &PathToFieldsNode,
    ) -> CallResult {
        let metadata_vec = self.bcc.sqmd_get_json_external(qlib_id, qhash, "")?;
        let metadata_val: &Value = &(serde_json::from_slice(&metadata_vec)?);

        let mut input = json!({});
        let response_vec = self.bcc.document_create(input)?;
        let response_val: serde_json::Value = serde_json::from_slice(&response_vec)?;
        let doc_id = match extract_body(response_val.clone()) {
            Some(o) => o.get("document-create-id").unwrap().as_u64(),
            None => {
                return self.bcc.make_error_with_kind(ErrorKinds::BadHttpParams(
                    "could not find key document-create-id",
                ))
            }
        }
        .unwrap();
        self.crawl_meta(metadata_val, path_to_fields_node, doc_id)?;
        input = json!({ "document_id": doc_id });
        Ok(Vec::new())
    }

    fn crawl_meta(
        &self,
        meta: &Value,
        path_to_field_node: &PathToFieldsNode,
        doc_id: u64,
    ) -> CallResult {
        // Check for fields that need to be added.
        let fields = path_to_field_node.get_fields();
        if fields.len() > 0 {
            for f in fields {
                match meta.get(&f.name) {
                    Some(field_content) => {
                        self.document_add_field(&f.name, &f.field_type, field_content, doc_id)?
                    }
                    None => (), // If field does not exist, skip
                }
            }
        }

        // Recurse down children of current PathToFieldsNode
        for (child_key, child_node) in self
            .path_to_fields_root
            .as_ref()
            .unwrap()
            .get_children_iter()
        {
            if child_key.eq("*") {
                /* Single asterisk-- visit every key in current metadata object. */
                for child_meta in meta.as_object().unwrap_or(&serde_json::Map::new()).values() {
                    self.crawl_meta(child_meta, child_node, doc_id)?;
                }
            } else if let Some(child_meta) = meta.get(child_key) {
                // Visit specified key in current metadata object if it exists.
                if let Some(link) = child_meta.get("/") {
                    // TODO: Handle link resolution. For now, we resolve all immediately instead of on the fly. Might be problemetic later if resolved files are super large.
                } else if child_meta.is_object() {
                    // Not link. Recurse down object without resolution.
                    self.crawl_meta(child_meta, child_node, doc_id)?;
                }
            }
        }
        Ok(Vec::new())
    }

    fn document_add_field(
        &self,
        field_name: &str,
        field_type: &str,
        field_content: &Value,
        doc_id: u64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        match field_type {
            "text" | "string" => {
                let input = json!({
                    "field": field_name,
                    "value": field_content.as_str(),
                    "doc": doc_id
                });
                self.bcc.document_add_text(input)?;
            }
            _ => {
                return Err(Box::new(ErrorKinds::Invalid(
                    "invalid field type encountered",
                )))
            }
        }
        Ok(())
    }
}
