# Search API

## Search using an Index Object

There are two kinds of search that can be performed on the fabric.

- Document-based search : results are documents (hash version + metadata prefix).
- Field-based search : results are single metadata fields.

To query an index, it is as simple as making a rep call on the Index Object using the function `search` or `search_field` with parameter `terms`.

`terms` has the following possible formats :

- `<keyword>` to make a global search regardless on the field name
- `f_<searchable_field_name>:=<keyword>` to restrict to search to the field `<searchable_field_name>`
- `AND` and `OR` are also possible, example : `f_synopsis:=Targaryen AND f_type:=episode`

Translated into an url, the document search performed on the fabric node `<URL>` using the Index Object `<QID>` in library `<ILIB>` with the query `f_synopsis:=Targaryen AND f_type:=episode` will have the following url :

`https://<URL>/qlibs/<LIB>/q/<QID>/rep/search?terms=f_synopsis%3A%3DTargaryen%20AND%20f_type%3A%3Depisode&authorization=<TOKEN>`

## What are the searchable fields of my Index Object ?

If you know the `<QID>` of that Index Object, you can retrieve the searchable fields running this command :

`curl -s 'https://<HOST>/qlibs/<LIB>/q/<QID>/meta/indexer/config/indexer/arguments/fields?authorization=<TOKEN>' | jq "keys"`

## Scripts

Some bash scripts have been added to help build the URLs.

- `bin/searchable-fields <HOST> <LIB> <QID> <TOKEN>` will return the list of fields that are searchable
- `bin/fields-stats <HOST> <LIB> <QID> <TOKEN>` will return global statistics about the fields (value histograms, min / max values, counts, unique, ...)
- `bin/document-search <HOST> <LIB> <QID> <TOKEN> "<QUERY>"` will perform a document search using the specified query (notice the quotes around `<QUERY>`)
- `bin/field-search <HOST> <LIB> <QID> <TOKEN> "<QUERY>"` will perform a field search using the specified query

## Pagination

Pagination can be used with three simple parameters :

- `start` is the index of the first result to return (default value is 0)
- `limit` is the maximum number of result to return (default value is `64`)
- `max_total` is the maximum total number of results that could be requested (`max_total` >= `limit`). It is useful to indicate to the server the number of results will never exceed a certain amount (default value is `null` (= no limit))

## Select

An additional parameter called `select` can be specified to select metadata subpaths (relative to the content) to resolve and embed in the query results. Several subpaths can be specified using comma-separated format. Example `select=/infos/cast,/infos/title`.

Additionaly, the select can be :

- `relative` (document centric) : The path do not start with a `/` and the selected metadata will end up in the `document` sub-tree of the result.
- `absolute` (content centric) : The path starts with a `/` and the selected metadata will end up in the `meta` sub-tree of the result.

## Sort

Fields can be sorted in ascending or descending order. **ONLY** `string`, `integer` and `unsigned` fields can be sorted. `text` fields cannot. The format for the `sort` parameter is : `sort=<field1>@asc,<field2>@desc,<field3>@asc,...`. If `asc` or `desc` is not present, `asc` is implied. Example : `sort=f_tile@desc`.

## Stats

Statistics about the current search query terms can be gathered using the `stats` parameters as such : `stats=<field1>,<field2>,<field3>,...`. That paramater will gather statistics about the specified **indexed** fields.

## Search results

### Field Search

The results of a field search will be a json list of items like the one below :

```json
{
  "pagination": {
    "limit": 64,
    "max_total": null,
    "start": 0,
    "total": 71
  },
  "results": [
    {
      "hash": "hq__XXXXX",
      "id": "iq__XXXXX",
      "path": "/infos/cast[0]/name",
      "qlib_id": "ilibXXXXX",
      "type": "hq__XXXXX",
      "value": "Keanu Reeves",
      "meta": {
        /* results of the absolute "select" are here */
      }
    },
    "..."
  ],
  "stats": {
    /* results of the "stats" option are here */
  }
}
```

where

- `hash` is the version hash of the object containing the field value
- `id` is the content id of the object containing the field value
- `qlib_id` is the library id containing the object
- `type` is the hash of the content type of the object
- `path` is the json path relative to the object containing it
- `value` is the value of that field
- `meta` is present and contains the selected subpaths of `hq__XXXXX` if `select` was provided
- `stats` contains the requested statistics using the `stats` option.

### Document Search

The results of a document search will be a json list of items like the one below :

```json
{
  "pagination": {
    "limit": 64,
    "max_total": null,
    "start": 0,
    "total": 47
  },
  "results": [
    {
      "hash": "hq__XXXXX",
      "id": "iq__XXXXX",
      "prefix": "/",
      "qlib_id": "ilibXXXXX",
      "type": "hq__XXXXX",
      "meta": {
        /* results of the "select" option are here */
      },
      "document": {
        /* results of the relative "select" are here */
      }
    },
    "..."
  ],
  "stats": {
    /* results of the "stats" option are here */
  }
}
```

where

- `hash` is the hash of the content that is found
- `id` is the content id of the content
- `qlib_id` is the library id containing the content
- `type` is the hash of the content type of the object
- `prefix` is the path of the indexed document inside the content (a prefix of `"/"` means the indexed document is the content).
- `meta` is present and contains the selected subpaths of `hq__XXXXX` if absolute `select` was provided
- `document` is present and contains the selected subpaths of `hq__XXXXX` if relative `select` was provided
- `stats` contains the requested statistics using the `stats` option.
