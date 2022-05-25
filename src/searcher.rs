use elvwasm::BitcodeContext;
use serde_json::{json, Value};
use wapc_guest::CallResult;

struct Searcher<'a, 'b> {
    bcc: &'a BitcodeContext<'b>,
}

impl<'a, 'b> Searcher<'a, 'b> {
    pub fn content_query(bcc: &BitcodeContext) -> () {
        let mut searcher = Searcher { bcc: bcc };
    }

    fn query(&self, query_str: &str) -> CallResult {
        // let hash_part_id_vec = self
        //     .bcc
        //     .sqmd_get_json(&format!("indexer/part/{}", part_name))?;
        // let hash_part_id = serde_json::from_slice(&hash_part_id_vec)?;
        let mut input = json!({});
        self.bcc.index_reader_builder_create(input)?;

        input = json!({});
        self.bcc.reader_builder_query_parser_create(input)?;

        input = serde_json::from_str(r#"{ "fields" : ["title", "body"] } }"#).unwrap();
        self.bcc.query_parser_for_index(input)?;

        input = json!({"query": query_str});
        self.bcc.query_parser_parse_query(input)?;

        input = json!({});
        let search_results = self.bcc.query_parser_search(input);

        Ok(Vec::new())
    }
}
