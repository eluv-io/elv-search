
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

## Pagination

Pagination can be used with three simple parameters :
* `start` is the index of the first result to return (default value is 0)
* `limit` is the maximum number of result to return (default value is `max_total`)
* `max_total` is the maximum total number of results that could be requested (`max_total` >= `limit`). It is useful to indicate to the server the number of results will never exceed a certain amount (default value is `2048`)

## Search results

### Field Search

The results of a field search will be a json list of items like the one below :

```json
{
  "hash": "hq__XXXXX",
  "path": "infos/cast[0]/name",
  "value": "Keanu Reeves",
  "links": [
    {
      "field": "actor_name",
      "namespace": "public.movies.infos.name",
      "path": "public/movies[42]/infos/cast[0]/name"
    },
    "...",
  ],
}
```

where
* `hash` is the version hash of the object containing the field value
* `path` is the json path relative to the object containing it
* `value` is the value of that field
* `links` contains all the links that have been found by the crawler that point to that field.
  * `field` is the name of the field as seen by the search engine (i.e. searchable field)
  * `namespace` are the metadata keys (from the root content metadata) that links to that field
  * `path` is the json path (from the root content metadata) that points to the field

### Content Search

The results of a content search will be a json list of items like the one below :

```json
{
  "hash": "hq__XXXXX",
  "fields": [
    {
      "path": "infos/cast[0]/name",
      "value": "Keanu Reeves",
      "links": [
        {
          "field": "actor_name",
          "namespace": "public.movies.infos.name",
          "path": "public/movies[42]/infos/cast[0]/name"
        },
        "...",
      ],
    },
    "..."
  ]
}
```

where
* `hash` is the hash of the content that is found
* `fields` contains all the fields that have been crawled on this content. The format of each entry in `field` is the same as the entry of a field search result (except for the missing `hash`).
