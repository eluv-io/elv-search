use elvwasm::{implement_bitcode_module, BitcodeContext, ErrorKinds};
use wapc_guest::CallResult;

/**
 * Perform crawl on index object. Index must have been built already.
 */
fn crawl(bcc: &mut BitcodeContext) -> CallResult {
    Ok(Vec::new())
}
