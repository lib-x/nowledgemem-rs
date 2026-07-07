use std::collections::BTreeSet;
use std::fs::File;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use progenitor::{GenerationSettings, Generator, InterfaceStyle};
use serde_json::{Map, Value};

fn main() {
    let src = std::env::var("NMEM_OPENAPI_PATH")
        .map(Into::into)
        .unwrap_or_else(|_| Path::new("openapi/nowledge-mem.openapi.json").to_path_buf());
    println!("cargo:rerun-if-changed={}", src.display());

    let file = File::open(&src).expect("open OpenAPI document");
    let mut spec: Value = serde_json::from_reader(file).expect("parse OpenAPI document");
    normalize_openapi_31_to_30(&mut spec);
    remove_unsupported_multipart_operations(&mut spec);
    normalize_multi_error_response_operations(&mut spec);
    disambiguate_inline_enum_titles(&mut spec);
    if std::env::var_os("NMEM_WRITE_NORMALIZED_OPENAPI").is_some() {
        std::fs::write(
            "/tmp/nowledgemem-normalized-openapi.json",
            serde_json::to_vec_pretty(&spec).expect("serialize normalized OpenAPI document"),
        )
        .expect("write normalized OpenAPI document");
    }
    let spec: openapiv3::OpenAPI =
        serde_json::from_value(spec).expect("decode normalized OpenAPI document");

    let mut generator = new_generator();
    let tokens = match catch_unwind(AssertUnwindSafe(|| generator.generate_tokens(&spec))) {
        Ok(Ok(tokens)) => tokens,
        Ok(Err(error)) => diagnose_generation_error(&spec, error),
        Err(_) => diagnose_generation_panic(&spec),
    };
    let ast = syn::parse2(tokens).expect("parse generated Rust client");
    let content = prettyplease::unparse(&ast);

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set by Cargo");
    let out_file = Path::new(&out_dir).join("codegen.rs");
    std::fs::write(out_file, content).expect("write generated Rust client");
}

fn normalize_openapi_31_to_30(spec: &mut Value) {
    if let Value::Object(root) = spec {
        root.insert("openapi".to_string(), Value::String("3.0.3".to_string()));
        root.remove("jsonSchemaDialect");
    }
    normalize_const_keywords(spec);
    normalize_nullable_type_arrays(spec);
    normalize_nullable_any_of(spec);
}

fn remove_unsupported_multipart_operations(spec: &mut Value) {
    let Some(paths) = spec.get_mut("paths").and_then(Value::as_object_mut) else {
        return;
    };
    for path_item in paths.values_mut() {
        let Some(methods) = path_item.as_object_mut() else {
            continue;
        };
        methods.retain(|_, operation| !is_multipart_operation(operation));
    }
    paths.retain(|_, path_item| {
        path_item
            .as_object()
            .is_some_and(|methods| !methods.is_empty())
    });
}

fn is_multipart_operation(operation: &Value) -> bool {
    operation
        .get("requestBody")
        .and_then(|request_body| request_body.get("content"))
        .and_then(Value::as_object)
        .is_some_and(|content| content.contains_key("multipart/form-data"))
}

fn normalize_multi_error_response_operations(spec: &mut Value) {
    let Some(paths) = spec.get_mut("paths").and_then(Value::as_object_mut) else {
        return;
    };
    for path_item in paths.values_mut() {
        let Some(methods) = path_item.as_object_mut() else {
            continue;
        };
        for operation in methods.values_mut() {
            let Some(responses) = operation
                .get_mut("responses")
                .and_then(Value::as_object_mut)
            else {
                continue;
            };
            let typed_error_count = responses
                .iter()
                .filter(|(status, response)| is_error_status(status) && has_json_schema(response))
                .count();
            if typed_error_count <= 1 {
                continue;
            }
            for (status, response) in responses {
                if is_error_status(status)
                    && let Some(response) = response.as_object_mut()
                {
                    response.remove("content");
                }
            }
        }
    }
}

fn is_error_status(status: &str) -> bool {
    status == "default" || status.starts_with('4') || status.starts_with('5')
}

fn has_json_schema(response: &Value) -> bool {
    response
        .get("content")
        .and_then(|content| content.get("application/json"))
        .and_then(|json| json.get("schema"))
        .is_some()
}

fn disambiguate_inline_enum_titles(spec: &mut Value) {
    let Some(schemas) = spec
        .get_mut("components")
        .and_then(|components| components.get_mut("schemas"))
        .and_then(Value::as_object_mut)
    else {
        return;
    };

    for (schema_name, schema) in schemas {
        let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) else {
            continue;
        };
        for (property_name, property_schema) in properties {
            let title = format!(
                "{}{}",
                to_pascal_case(schema_name),
                to_pascal_case(property_name)
            );
            set_enum_title(property_schema, &title);
        }
    }
}

fn set_enum_title(schema: &mut Value, title: &str) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    if object.contains_key("enum") {
        object.insert("title".to_string(), Value::String(title.to_string()));
    }
    if let Some(any_of) = object.get_mut("anyOf").and_then(Value::as_array_mut) {
        for subschema in any_of {
            set_enum_title(subschema, title);
        }
    }
    if let Some(one_of) = object.get_mut("oneOf").and_then(Value::as_array_mut) {
        for subschema in one_of {
            set_enum_title(subschema, title);
        }
    }
}

fn to_pascal_case(value: &str) -> String {
    let mut result = String::new();
    let mut uppercase_next = true;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if uppercase_next {
                result.push(ch.to_ascii_uppercase());
                uppercase_next = false;
            } else {
                result.push(ch);
            }
        } else {
            uppercase_next = true;
        }
    }
    result
}

fn normalize_const_keywords(value: &mut Value) {
    match value {
        Value::Array(values) => {
            for value in values {
                normalize_const_keywords(value);
            }
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                normalize_const_keywords(value);
            }
            if let Some(const_value) = object.remove("const") {
                object
                    .entry("enum")
                    .or_insert_with(|| Value::Array(vec![const_value]));
            }
        }
        _ => {}
    }
}

fn normalize_nullable_type_arrays(value: &mut Value) {
    match value {
        Value::Array(values) => {
            for value in values {
                normalize_nullable_type_arrays(value);
            }
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                normalize_nullable_type_arrays(value);
            }

            let Some(types) = object.get("type").and_then(Value::as_array) else {
                return;
            };
            let non_null_types = types
                .iter()
                .filter(|value| value.as_str() != Some("null"))
                .cloned()
                .collect::<Vec<_>>();
            if non_null_types.len() != 1 || non_null_types.len() == types.len() {
                return;
            }

            object.insert("type".to_string(), non_null_types[0].clone());
            object.insert("nullable".to_string(), Value::Bool(true));
        }
        _ => {}
    }
}

fn normalize_nullable_any_of(value: &mut Value) {
    match value {
        Value::Array(values) => {
            for value in values {
                normalize_nullable_any_of(value);
            }
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                normalize_nullable_any_of(value);
            }

            let Some(any_of) = object.get("anyOf").and_then(Value::as_array) else {
                return;
            };
            if any_of.len() != 2 {
                return;
            }

            let null_index = any_of.iter().position(is_null_schema);
            let Some(null_index) = null_index else {
                return;
            };
            let non_null_index = 1 - null_index;
            let mut schema = any_of[non_null_index].clone();
            set_nullable(&mut schema);

            let outer = std::mem::take(object);
            let mut merged = schema_to_object(schema);
            for (key, value) in outer {
                if key != "anyOf" {
                    merged.insert(key, value);
                }
            }
            *object = merged;
        }
        _ => {}
    }
}

fn is_null_schema(value: &Value) -> bool {
    value
        .as_object()
        .and_then(|object| object.get("type"))
        .and_then(Value::as_str)
        == Some("null")
}

fn set_nullable(schema: &mut Value) {
    let Value::Object(object) = schema else {
        return;
    };
    if object.contains_key("$ref") {
        let reference = Value::Object(std::mem::take(object));
        object.insert("allOf".to_string(), Value::Array(vec![reference]));
    }
    object.insert("nullable".to_string(), Value::Bool(true));
}

fn schema_to_object(schema: Value) -> Map<String, Value> {
    match schema {
        Value::Object(object) => object,
        other => {
            let mut object = Map::new();
            object.insert("schema".to_string(), other);
            object
        }
    }
}

fn diagnose_generation_error(spec: &openapiv3::OpenAPI, error: progenitor::Error) -> ! {
    if let Some(components) = &spec.components {
        let schema_values = components
            .schemas
            .iter()
            .map(|(name, schema)| {
                (
                    name.to_string(),
                    serde_json::to_value(schema).expect("serialize OpenAPI schema"),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();

        for (name, _) in &components.schemas {
            let mut closure = BTreeSet::new();
            collect_schema_refs(name, &schema_values, &mut closure);

            let mut minimal = spec.clone();
            minimal.paths = openapiv3::Paths::default();
            let mut components = openapiv3::Components::default();
            for dependency in &closure {
                let Some(schema) = spec
                    .components
                    .as_ref()
                    .and_then(|components| components.schemas.get(dependency))
                else {
                    panic!(
                        "generate Rust client from OpenAPI document: {error:?}; schema {name} references missing schema {dependency}"
                    );
                };
                components
                    .schemas
                    .insert(dependency.clone(), schema.clone());
            }
            minimal.components = Some(components);
            let mut generator = new_generator();
            match catch_unwind(AssertUnwindSafe(|| generator.generate_tokens(&minimal))) {
                Ok(Ok(_)) => {}
                Ok(Err(schema_error)) => {
                    panic!(
                        "generate Rust client from OpenAPI document: {error:?}; first failing schema closure rooted at {name}: {schema_error:?}; closure={closure:?}"
                    );
                }
                Err(_) => {
                    panic!(
                        "generate Rust client from OpenAPI document: {error:?}; first panicking schema closure rooted at {name}; closure={closure:?}"
                    );
                }
            }
        }
    }
    for (path, item) in &spec.paths.paths {
        let item_value = serde_json::to_value(item).expect("serialize OpenAPI path item");
        let mut closure = BTreeSet::new();
        for reference in schema_refs(&item_value) {
            collect_schema_refs(&reference, &component_schema_values(spec), &mut closure);
        }

        let mut minimal = spec.clone();
        minimal.paths = openapiv3::Paths::default();
        minimal.paths.paths.insert(path.clone(), item.clone());
        let mut components = openapiv3::Components::default();
        for dependency in &closure {
            let Some(schema) = spec
                .components
                .as_ref()
                .and_then(|components| components.schemas.get(dependency))
            else {
                panic!(
                    "generate Rust client from OpenAPI document: {error:?}; path {path} references missing schema {dependency}"
                );
            };
            components
                .schemas
                .insert(dependency.clone(), schema.clone());
        }
        minimal.components = Some(components);
        let mut generator = new_generator();
        match catch_unwind(AssertUnwindSafe(|| generator.generate_tokens(&minimal))) {
            Ok(Ok(_)) => {}
            Ok(Err(path_error)) => {
                panic!(
                    "generate Rust client from OpenAPI document: {error:?}; first failing path {path}: {path_error:?}"
                );
            }
            Err(_) => {
                panic!(
                    "generate Rust client from OpenAPI document: {error:?}; first panicking path {path}; closure={closure:?}"
                );
            }
        }
    }
    diagnose_incremental_paths(spec, &format!("{error:?}"));
    panic!("generate Rust client from OpenAPI document: {error:?}");
}

fn diagnose_generation_panic(spec: &openapiv3::OpenAPI) -> ! {
    let schema_values = component_schema_values(spec);
    for (path, item) in &spec.paths.paths {
        let item_value = serde_json::to_value(item).expect("serialize OpenAPI path item");
        let mut closure = BTreeSet::new();
        for reference in schema_refs(&item_value) {
            collect_schema_refs(&reference, &schema_values, &mut closure);
        }

        let mut minimal = spec.clone();
        minimal.paths = openapiv3::Paths::default();
        minimal.paths.paths.insert(path.clone(), item.clone());
        let mut components = openapiv3::Components::default();
        for dependency in &closure {
            let Some(schema) = spec
                .components
                .as_ref()
                .and_then(|components| components.schemas.get(dependency))
            else {
                panic!(
                    "generate Rust client from OpenAPI document panicked; path {path} references missing schema {dependency}"
                );
            };
            components
                .schemas
                .insert(dependency.clone(), schema.clone());
        }
        minimal.components = Some(components);
        let mut generator = new_generator();
        match catch_unwind(AssertUnwindSafe(|| generator.generate_tokens(&minimal))) {
            Ok(Ok(_)) => {}
            Ok(Err(path_error)) => {
                panic!(
                    "generate Rust client from OpenAPI document panicked; first failing path {path}: {path_error:?}; closure={closure:?}"
                );
            }
            Err(_) => {
                panic!(
                    "generate Rust client from OpenAPI document panicked; first panicking path {path}; closure={closure:?}"
                );
            }
        }
    }
    panic!("generate Rust client from OpenAPI document panicked");
}

fn diagnose_incremental_paths(spec: &openapiv3::OpenAPI, original_error: &str) {
    let schema_values = component_schema_values(spec);
    let mut accumulated = spec.clone();
    accumulated.paths = openapiv3::Paths::default();
    accumulated.components = Some(openapiv3::Components::default());
    let mut accumulated_closure = BTreeSet::new();

    for (path, item) in &spec.paths.paths {
        let item_value = serde_json::to_value(item).expect("serialize OpenAPI path item");
        for reference in schema_refs(&item_value) {
            collect_schema_refs(&reference, &schema_values, &mut accumulated_closure);
        }
        accumulated.paths.paths.insert(path.clone(), item.clone());
        let mut components = openapiv3::Components::default();
        for dependency in &accumulated_closure {
            let Some(schema) = spec
                .components
                .as_ref()
                .and_then(|components| components.schemas.get(dependency))
            else {
                panic!(
                    "generate Rust client from OpenAPI document: {original_error}; accumulated path {path} references missing schema {dependency}"
                );
            };
            components
                .schemas
                .insert(dependency.clone(), schema.clone());
        }
        accumulated.components = Some(components);

        let mut generator = new_generator();
        match catch_unwind(AssertUnwindSafe(|| generator.generate_tokens(&accumulated))) {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                panic!(
                    "generate Rust client from OpenAPI document: {original_error}; adding path {path} makes accumulated generation fail: {error:?}; paths={}",
                    accumulated.paths.paths.len()
                );
            }
            Err(_) => {
                panic!(
                    "generate Rust client from OpenAPI document: {original_error}; adding path {path} makes accumulated generation panic; paths={}",
                    accumulated.paths.paths.len()
                );
            }
        }
    }
}

fn new_generator() -> Generator {
    let mut settings = GenerationSettings::default();
    let settings = settings
        .with_interface(InterfaceStyle::Builder)
        .with_inner_type(quote::quote! { crate::ClientState })
        .with_pre_hook_async(quote::quote! { crate::apply_request_options });
    Generator::new(settings)
}

fn component_schema_values(spec: &openapiv3::OpenAPI) -> std::collections::BTreeMap<String, Value> {
    spec.components
        .as_ref()
        .map(|components| {
            components
                .schemas
                .iter()
                .map(|(name, schema)| {
                    (
                        name.to_string(),
                        serde_json::to_value(schema).expect("serialize OpenAPI schema"),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn collect_schema_refs(
    name: &str,
    schemas: &std::collections::BTreeMap<String, Value>,
    visited: &mut BTreeSet<String>,
) {
    if !visited.insert(name.to_string()) {
        return;
    }
    let Some(schema) = schemas.get(name) else {
        return;
    };
    for reference in schema_refs(schema) {
        collect_schema_refs(&reference, schemas, visited);
    }
}

fn schema_refs(value: &Value) -> Vec<String> {
    let mut refs = Vec::new();
    collect_schema_refs_from_value(value, &mut refs);
    refs
}

fn collect_schema_refs_from_value(value: &Value, refs: &mut Vec<String>) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_schema_refs_from_value(value, refs);
            }
        }
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(Value::as_str)
                && let Some(name) = reference.strip_prefix("#/components/schemas/")
            {
                refs.push(name.to_string());
            }
            for value in object.values() {
                collect_schema_refs_from_value(value, refs);
            }
        }
        _ => {}
    }
}
