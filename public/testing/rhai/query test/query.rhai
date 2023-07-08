let value = get(hashmap, "user");

if value == "" {
    return "<pre>no user set, set with query parameter\nexample: /query.rhai?user=(username)</pre>";
}

"hello " + value