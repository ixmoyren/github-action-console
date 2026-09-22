# The repo's release manifest is the source of truth; SQLite holds pointers and caches

Definitions of 发布目标 and 打包配置 live in a YAML release manifest inside each repository, because the CI runner reads its configuration from the repo at the dispatched ref and collaborators must see the same definitions. SQLite stores only channel pointers, cached GitHub data, and non-versioned preferences. Considered making SQLite the source of truth and passing the whole manifest as a workflow input (rejected: the repo side keeps no recipe, and the flat-input path is unreachable), and storing only the
channel pointers locally while definitions lived elsewhere (rejected: two homes for one concept).
