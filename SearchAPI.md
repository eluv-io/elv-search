
# Search API

## Search using an Index Object

There are two kinds of search that can be performed on the fabric.
* Content-based search : results are contents
* Field-based search : results are single metadata fields

To query an index, it is as simple as making a rep call on the Index Object using the function `md_content_search` or `md_field_search` with parameter `query`.

`query` has the following possible formats :
  * `<keyword>` to make a global search regardless on the field name
  * `f_<searchable_field_name>:=<keyword>` to restrict to search to the field `<searchable_field_name>`
  * `AND` and `OR` are also possible, example : `f_synopsis:=Targaryen AND f_type:=episode`

  Translated into an url, the content search performed on the fabric node `<HOST>` using the Index Object `<QID>` in library `<ILIB>` with the query `f_synopsis:=Targaryen AND f_type:=episode` will have the following url :

  `https://<HOST>/qlibs/<LIB>/q/<QID>/rep/md_content_search?query=f_synopsis%3A%3DTargaryen%20AND%20f_type%3A%3Depisode&authorization=<TOKEN>`

  ## What are the searchable fields of my Index Object ?

  If you know the `<QID>` of that Index Object, you can retrieve the searchable fields running this command :

  `curl -s 'https://<HOST>/qlibs/<LIB>/q/<QID>/meta/indexer/config/indexer/arguments/fields?authorization=<TOKEN>' | jq "keys"`
  
  ## Scripts

  Some bash scripts have been added to help build the URLs.

  * `bin/searchable-fields <HOST> <LIB> <QID> <TOKEN>` will return the list of fields that are searchable
  * `bin/content-search <HOST> <LIB> <QID> <TOKEN> "<QUERY>"` will perform a content search using the specified query (notice the quotes around `<QUERY>`)
  * `bin/field-search <HOST> <LIB> <QID> <TOKEN> "<QUERY>"` will perform a field search using the specified query