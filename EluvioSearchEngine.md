# How to create and use a Search Engine on the Eluvio platform ?

A private search engine can be created and used on the eluvio fabric plateform. To enable that, you need to create an Index Object (cf. below) that points to a Root Content containing the root metadata. We call it the root metadata because it can contain links to other metadata, and those metadata will be crawled following those links recursively.

## Before Creating an Index Object

An index object is an object that needs two things :

- A reference to a content type having the `builtin` capabilities (creating simple content type with metadata `{"bitcode_format":"builtin"}` will suffice).
- A metadata having a particular field and format (which is used to configure the crawler and search engine). That metadata should contain the field `.indexer.config` and the format of that field should follow the following rules :
  - `fabric.root.library` and `fabric.root.content` should correspond to the library ID and content ID of the root metadata to crawl.
  - `fabric.policy.paths` should contain a list of content-centric metadata paths to policy-filter after the results have been augmented (through the use of a `select`)
  - `fabric.policy.disable_document_post_filtering` should indicate wether to run the default policy-filtering on returned documents.
  - `indexer.type` should be equal to `"metadata-text"`
  - `indexer.arguments.document.prefix` defines the prefix for a searchable document (eg. `/` for content-document and `/assets/*` for asset focument). Any field found by the crawler having the same prefix will be aggregated to form one searchable document.
  - `indexer.arguments.query.suffix` is a string that will be concatenated to every search query.
  - `indexer.arguments.fields` will contain all the fields that are searchable.
  - A searchable field inside `indexer.arguments.fields` should have the following format :
  ```json
  "<searchable_field_name>": {
    "options": null,
    "paths": [
        "<path_0>",
        "<path_1>",
        "...",
        "<path_N>"
    ],
    "type": "<type>"
  }
  ```
  - `<searchable_field_name>` can be any string, this name will be used when querying the index for that particular field (cf. below).
  - `<type>` can be one of the following value :
    - `"text"`: Used for indexing large text and provides non-perfect-match search capability.
    - `"string"`: Used for labels, categories, uid, ... any simple string that requires perfect matching. This field also provides the computation of an histogram in the results of a search.
    - `"integer"` or `"unsigned"`: Used for indexing integer number.
  - `<path_i>` correspond to all the metadata paths of leaf fields to index under the name `<searchable_field_name>`.
  - For example :
  ```json
  "synopsis": {
    "options": null,
    "paths": [
        "public.movies.*.synopsis",
        "public.series.*.synopsis",
        "public.episodes.*.synopsis",
    ],
    "type": "text"
  }
  ```
  will index all the fields it can find corresponding to one of the paths and index them under the name `synopsis`. When doing a search, it will be possible to query a synopsis using a query string like this `f_synopsis:=<keyword>` (cf. possible queries below).
  - Paths can have a wildcard `*`, meaning that any key name will be crawled.
  - Paths are namespaces in the sense that arrays are ignored. For example : Metadata `A.B` and `A[0].B` will both be captured by path `A.B`.

Here is an example of a proper metadata for the Index Object :

```json
{
  "public": {
    "name": "Index - My Site"
  },
  "indexer": {
    "config": {
      "fabric": {
        "root": {
          "library": "ilib2XX6yS5S8bgAeLVxDGKeoXcNVc2N",
          "content": "iq__CWJC7xQ9v2rXPYMkyhRF27Bf1rj"
        },
        "policy": {
          "paths": ["/offerings/*"],
          "disable_document_post_filtering": true
        }
      },
      "indexer": {
        "type": "metadata-text",
        "arguments": {
          "fields": {
            "title": {
              "options": null,
              "paths": [
                "public.movies.*.title",
                "public.series.*.title",
                "public.episodes.*.title"
              ]
            },
            "type": {
              "options": null,
              "paths": [
                "public.movies.*.type",
                "public.series.*.type",
                "public.episodes.*.type"
              ]
            },
            "synopsis": {
              "options": null,
              "paths": [
                "public.movies.*.synopsis",
                "public.series.*.synopsis",
                "public.episodes.*.synopsis"
              ]
            }
          }
        }
      }
    }
  }
}
```

## Creating an Index Object

Once both the content type and metadata are ready (cf. above). It's time to create the index object using the fabric. The steps are as such :

1. Create a new content by giving it the type hash of the content type, and the metadata prepared above.
1. Finalize that object.

At that point, the index is empty and cannot be searched. In order to be usable, it needs to crawl (next step).

## Crawling using an Index Object

Any time your root metadata changes, you need to recrawl again in order to update the index. To do that, follow those steps :

1. Edit the Index Object (it will give you a write token)
1. Make a bitcode call to `search_update` in non-constant mode and using the write token. This step returns a LRO handle.
1. In order to know wether the update finished, you need to call the `crawl_status` bitcode on the write token and put the LRO handle inside the body of the request (eg. `{ lro_handle: 'b2e11708-c023-4b79-8ae5-c5590e6c285e' }`).
1. Once the status is set to "terminated", the write token has to be finalized.

## Simple Search using an Index Object

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

For more informations about the search features, please have a look here : [Search API](./SearchAPI.md)
