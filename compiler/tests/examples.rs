use std::fs;
use std::process::{Command, Output};

fn example_path(rel: &str) -> String {
    format!("{}/../examples/{}", env!("CARGO_MANIFEST_DIR"), rel)
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ostrinc"))
        .args(args)
        .output()
        .expect("failed to run ostrinc")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

#[test]
fn hello_type_checks_ok() {
    let out = run(&[&example_path("hello.ostrin")]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("OK"));
}

#[test]
fn record_fields_are_checked_and_generic_fields_are_substituted() {
    let out = run(&["--run", &example_path("field_access.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["9", "seven"]);
}

#[test]
fn invalid_record_field_access_and_assignment_are_rejected() {
    let out = run(&[&example_path("field_access_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1043"), "missing field diagnostic: {err}");
    assert!(err.contains("missing"), "missing field name: {err}");
    assert!(err.contains("E1041"), "missing field assignment diagnostic: {err}");
    assert!(err.contains("String") && err.contains("Int"), "missing field types: {err}");
}

#[test]
fn named_and_default_function_arguments_work() {
    let out = run(&["--run", &example_path("function_arguments.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["Ostrin!", "Ostrin?"]);
}

#[test]
fn function_argument_count_names_and_types_are_checked() {
    let out = run(&[&example_path("function_argument_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1041"), "missing argument diagnostic: {err}");
    assert!(err.contains("Missing required argument 'value'"), "missing arity diagnostic: {err}");
    assert!(err.contains("String") && err.contains("Int"), "missing argument types: {err}");
    assert!(err.contains("no parameter named 'extra'"), "missing named argument diagnostic: {err}");
    assert!(err.contains("Positional arguments must come before named arguments"), "missing ordering diagnostic: {err}");
}

#[test]
fn collection_lookup_results_are_option_types() {
    let out = run(&[&example_path("collection_types_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1041"), "missing Option argument diagnostic: {err}");
    assert!(err.contains("Option<Int>"), "collection lookup did not preserve Option<Int>: {err}");
}

#[test]
fn task_channel_and_iterator_types_are_checked() {
    let out = run(&[&example_path("concurrency_types_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1041"), "missing concurrency type diagnostic: {err}");
    assert!(err.contains("Method 'send' expects 'Int', got 'String'"), "missing channel send diagnostic: {err}");
    assert!(err.contains("Option<Int>"), "missing channel receive type: {err}");
    assert!(err.contains("String"), "missing task join type: {err}");
}

#[test]
fn collection_method_arguments_are_checked() {
    let out = run(&[&example_path("collection_argument_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1041"), "missing collection argument diagnostic: {err}");
    assert!(err.contains("push") && err.contains("expects 'Int'") && err.contains("String"), "missing List argument types: {err}");
    assert!(err.contains("get") && err.contains("expects 'String'") && err.contains("Int"), "missing Map key type: {err}");
    assert!(err.contains("add") && err.contains("expects 'Int'"), "missing Set element type: {err}");
}

#[test]
fn control_flow_conditions_and_explicit_returns_are_checked() {
    let out = run(&[&example_path("control_type_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("If condition expects 'Bool', got 'Int'"), "missing if condition diagnostic: {err}");
    assert!(err.contains("While condition expects 'Bool', got 'String'"), "missing while condition diagnostic: {err}");
    assert!(err.contains("Logical operators expect 'Bool' operands"), "missing logical operand diagnostic: {err}");
    assert!(err.contains("Return expression expects 'Int', got 'String'"), "missing return type diagnostic: {err}");
    assert!(err.contains("Empty return expects function return type 'Void'"), "missing empty return diagnostic: {err}");
    assert!(err.contains("Default value for 'value' expects 'Int', got 'String'"), "missing default value diagnostic: {err}");
}

#[test]
fn option_and_result_methods_run() {
    let out = run(&["--run", &example_path("option_result.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(
        stdout(&out).lines().collect::<Vec<_>>(),
        [
            "true", "Some(5)", "Some(5)", "4", "Ok(4)", "true", "9",
            "Err(missing)", "true", "Ok(8)", "Some(7)", "true", "Err(bad!)", "None"
        ]
    );
}

#[test]
fn option_and_result_lambdas_receive_contextual_types() {
    let out = run(&["--members", "--json", &example_path("option_result_types.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("\"name\":\"mapped\",\"type\":\"Option<Int>\""));
    assert!(text.contains("\"name\":\"chained\",\"type\":\"Option<Int>\""));
    assert!(text.contains("\"name\":\"result_mapped\",\"type\":\"Result<Int, String>\""));
    assert!(text.contains("\"name\":\"error_mapped\",\"type\":\"Result<Int, String>\""));
}

#[test]
fn option_and_result_method_arguments_are_checked() {
    let out = run(&[&example_path("option_result_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1041"), "missing Option/Result diagnostic: {err}");
    assert!(err.contains("unwrap_or") && err.contains("expects 'Int'") && err.contains("String"), "missing unwrap_or type diagnostic: {err}");
    assert!(err.contains("map") && err.contains("fn(Int)"), "missing map callback-shape diagnostic: {err}");
}

#[test]
fn try_unwraps_and_propagates_option_and_result() {
    let out = run(&["--run", &example_path("try_result.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(
        stdout(&out).lines().collect::<Vec<_>>(),
        ["Ok(4)", "Err(converted: bad)", "Some(10)", "None"]
    );
}

#[test]
fn try_requires_a_matching_enclosing_result_type() {
    let out = run(&[&example_path("try_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1041"), "missing try diagnostic: {err}");
    assert!(err.contains("enclosing Option/Result"), "missing container mismatch: {err}");
    assert!(err.contains("propagates error type") && err.contains("String") && err.contains("Int"), "missing error propagation mismatch: {err}");
    assert!(err.contains("try catch") && err.contains("fn(String)"), "missing catch signature mismatch: {err}");
}

#[test]
fn standard_library_file_io_and_parsing_work() {
    let out = run(&["--run", &example_path("stdlib_io.ostrin")]);
    let _ = fs::remove_file("target/ostrin-stdlib-test.txt");
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["true", "42", "hello from Ostrin"]);
}

#[test]
fn standard_library_arguments_are_checked() {
    let out = run(&[&example_path("stdlib_io_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1041"), "missing standard library diagnostic: {err}");
    assert!(err.contains("read_file") && err.contains("String") && err.contains("Int"), "missing read_file type diagnostic: {err}");
    assert!(err.contains("write_file") && err.contains("String") && err.contains("Int"), "missing write_file type diagnostic: {err}");
    assert!(err.contains("parse_int") && err.contains("Bool"), "missing parse_int type diagnostic: {err}");
}

#[test]
fn json_diagnostics_are_editor_friendly_and_keep_source_locations() {
    let out = run(&["--json", &example_path("field_access_errors.ostrin")]);
    assert!(!out.status.success());
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 4, "expected one JSON object per diagnostic: {text}");
    assert!(lines.iter().all(|line| line.starts_with('{') && line.ends_with('}')));
    assert!(text.contains("\"code\":\"E1043\""));
    assert!(text.contains("\"message\":\"Type 'Score' has no field 'missing'.\""));
    assert!(text.contains("\"line\":8"));
    assert!(text.contains("\"column\":5"));
}

#[test]
fn cli_exposes_help_and_version() {
    let version = run(&["--version"]);
    assert!(version.status.success());
    assert!(stdout(&version).contains("ostrinc 0.1.0"));

    let help = run(&["--help"]);
    assert!(help.status.success());
    assert!(stdout(&help).contains("--json"));
    assert!(stdout(&help).contains("--check"));
    assert!(stdout(&help).contains("--symbols"));
    assert!(stdout(&help).contains("--members"));
    assert!(stdout(&help).contains("--types"));
}

#[test]
fn compiler_exports_symbols_and_signatures_for_editor_tools() {
    let out = run(&["--symbols", "--json", &example_path("advanced.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("\"kind\":\"function\""));
    assert!(text.contains("\"name\":\"double\""));
    assert!(text.contains("fn double<D: Dimension>(x: Quantity<D>) -> Quantity<D>"));
    assert!(text.contains("\"line\":1"));
}

#[test]
fn compiler_exports_type_members_and_local_bindings_for_editor_tools() {
    let out = run(&["--members", "--json", &example_path("collections.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("\"memberKind\":\"method\",\"owner\":\"List\",\"name\":\"push\""));
    assert!(text.contains("\"owner\":\"Map\",\"name\":\"get\""));
    assert!(text.contains("\"kind\":\"binding\",\"name\":\"numbers\",\"type\":\"List<Int>\",\"function\":\"main\",\"scopeDepth\":1"));
    assert!(text.contains("\"name\":\"doubled\",\"type\":\"List<Int>\""));
    assert!(text.contains("\"name\":\"evens\",\"type\":\"List<Int>\""));
    assert!(text.contains("\"name\":\"total\",\"type\":\"Int\""));
    assert!(text.contains("\"name\":\"found\",\"type\":\"Option<Int>\""));

    let advanced = run(&["--members", "--json", &example_path("advanced.ostrin")]);
    assert!(advanced.status.success(), "stderr: {}", stderr(&advanced));
    let advanced_text = stdout(&advanced);
    assert!(advanced_text.contains("\"name\":\"x\",\"type\":\"Quantity<D>\",\"function\":\"double\",\"scopeDepth\":0"));

    let generics = run(&["--members", "--json", &example_path("generics.ostrin")]);
    assert!(generics.status.success(), "stderr: {}", stderr(&generics));
    let generics_text = stdout(&generics);
    assert!(generics_text.contains("\"name\":\"a\",\"type\":\"Score\",\"function\":\"main\",\"scopeDepth\":1"));
    assert!(generics_text.contains("\"owner\":\"Score\",\"name\":\"value\""));
    assert!(generics_text.contains("\"resultType\":\"Int\""));
    assert!(generics_text.contains("\"line\":5,\"column\":1"));
}

#[test]
fn compiler_exports_inferred_expression_types_for_editor_tools() {
    let out = run(&["--types", "--json", &example_path("option_result_types.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("\"kind\":\"expression\""));
    assert!(text.contains("\"type\":\"Option<Int>\""));
    assert!(text.contains("\"type\":\"Result<Int, String>\""));
    assert!(text.contains("\"line\":2,\"column\":13"));
    assert!(text.contains("\"endLine\":2,\"endColumn\":20"));
}

#[test]
fn advanced_type_checks_ok() {
    let out = run(&[&example_path("advanced.ostrin")]);
    assert!(out.status.success());
}

#[test]
fn newlines_type_checks_ok() {
    let out = run(&[&example_path("newlines.ostrin")]);
    assert!(out.status.success());
}

#[test]
fn shapes_type_checks_ok() {
    let out = run(&[&example_path("shapes.ostrin")]);
    assert!(out.status.success());
}

#[test]
fn dimensional_errors_are_all_reported() {
    let out = run(&[&example_path("errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1024"), "missing E1024 in: {err}");
    assert!(err.contains("E1025"), "missing E1025 in: {err}");
    assert!(err.contains("E1001"), "missing E1001 in: {err}");
    assert_eq!(err.matches("E1024").count(), 2, "expected E1024 to appear twice: {err}");
}

#[test]
fn physics_runs_with_correct_unit_arithmetic() {
    let out = run(&["--run", &example_path("physics.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 5);
    assert_eq!(lines[1], "5 m/s");
    assert_eq!(lines[2], "6 kg");
    assert_eq!(lines[3], "400000000");
    assert_eq!(lines[4], "15");
}

#[test]
fn fibonacci_iterator_protocol_works() {
    let out = run(&["--run", &example_path("fibonacci.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let expected = ["0", "1", "1", "2", "3", "5", "8", "13", "21", "34"];
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines, expected);
}

#[test]
fn traits_dispatch_operators_via_impl() {
    let out = run(&["--run", &example_path("traits.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines, ["Vector2 { x: 4, y: 6 }", "true", "false", "true", "false", "true"]);
}

#[test]
fn derive_eq_and_ord_work_without_manual_impl() {
    let out = run(&["--run", &example_path("derive.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines, ["true", "false", "true", "true", "true"]);
}

#[test]
fn dyn_trait_dispatches_polymorphically() {
    let out = run(&["--run", &example_path("dyn_trait.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines, ["12.56636", "9", "3.14159", "24.70795"]);
}

#[test]
fn concurrency_channel_and_task_work() {
    let out = run(&["--run", &example_path("concurrency.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines, ["1", "2", "3", "4", "5", "7"]);
}

#[test]
fn spawn_capturing_mut_is_rejected() {
    let out = run(&[&example_path("concurrency_errors.ostrin")]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("E1100"));
}

#[test]
fn moved_channel_value_cannot_be_reused() {
    let out = run(&["--run", &example_path("moved_after_send.ostrin")]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("moved"));
}

#[test]
fn collections_map_set_and_combinators_work() {
    let out = run(&["--run", &example_path("collections.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        [
            "Some(1.5)", "false", "Some(12)", "2", "3", "true", "false", "2",
            "[2, 4, 6, 8, 10, 12]", "[2, 4, 6]", "21", "Some(4)", "true", "true"
        ]
    );
}

#[test]
fn list_mutation_is_shared_through_aliases() {
    let out = run(&["--run", &example_path("list_mutation.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines, ["2", "[1, 3, 4]", "[1, 3, 4]", "3"]);
}

#[test]
fn list_mutation_requires_mutable_binding() {
    let source = example_path("list_mutation_error.ostrin");
    let out = run(&[&source]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1053"), "missing E1053 in: {err}");
    assert!(err.contains("immutable binding 'numbers'"), "unexpected error: {err}");
}

#[test]
fn exhaustive_enum_match_checks_and_runs() {
    let out = run(&["--run", &example_path("match_exhaustive.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["red", "yellow", "green"]);
}

#[test]
fn non_exhaustive_enum_match_is_rejected() {
    let out = run(&[&example_path("match_non_exhaustive.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1060"), "missing E1060 in: {err}");
    assert!(err.contains("Green"), "missing variant in: {err}");
}

#[test]
fn generic_functions_infer_types_and_honor_trait_bounds() {
    let out = run(&["--run", &example_path("generics.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["true", "false", "7"]);
}

#[test]
fn generic_trait_bound_is_rejected_at_call_site() {
    let out = run(&[&example_path("generics_bound_error.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1042"), "missing E1042: {err}");
    assert!(err.contains("Plain"), "missing concrete type in: {err}");
    assert!(err.contains("Comparable"), "missing bound in: {err}");
}

#[test]
fn nested_enum_and_record_patterns_run() {
    let out = run(&["--run", &example_path("match_nested.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["42", "0", "5"]);
}

#[test]
fn malformed_match_patterns_are_rejected() {
    let out = run(&[&example_path("match_pattern_error.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1061"), "missing E1061: {err}");
    assert!(err.contains("other"), "missing unknown field diagnostic: {err}");
    assert!(err.contains("Pattern literal has type 'Bool'"), "missing nested type diagnostic: {err}");
    assert!(err.contains("carries data"), "missing bare constructor diagnostic: {err}");
}

#[test]
fn generic_enum_and_record_patterns_are_instantiated() {
    let out = run(&["--run", &example_path("generic_nested_patterns.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(
        stdout(&out).lines().collect::<Vec<_>>(),
        ["9", "7", "good", "failed", "empty", "4"]
    );
}

#[test]
fn nested_generic_match_reports_missing_inner_combination() {
    let out = run(&[&example_path("generic_nested_patterns_error.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1060"), "missing E1060: {err}");
    assert!(err.contains("Just"), "missing partially-covered variant: {err}");
}

#[test]
fn trait_supertraits_and_default_methods_dispatch() {
    let out = run(&["--run", &example_path("traits_defaults.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "7");
}

#[test]
fn trait_required_methods_supertraits_and_signatures_are_checked() {
    let out = run(&[&example_path("traits_semantics_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1050"), "missing supertrait diagnostic: {err}");
    assert!(err.contains("E1054"), "missing signature diagnostic: {err}");
    assert!(err.contains("E1055"), "missing required-method diagnostic: {err}");
}

#[test]
fn trait_supertraits_are_transitive_and_collisions_are_rejected() {
    let out = run(&[&example_path("traits_coherence_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1050"), "missing transitive-supertrait diagnostic: {err}");
    assert!(err.contains("Root"), "missing transitive supertrait name: {err}");
    assert!(err.contains("E1057"), "missing inherited-method collision diagnostic: {err}");
}

#[test]
fn trait_orphan_rule_rejects_foreign_trait_for_foreign_type() {
    let out = run(&[&example_path("proj_orphan/main.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1056"), "missing orphan-rule diagnostic: {err}");
    assert!(err.contains("ForeignTrait"), "missing foreign trait name: {err}");
    assert!(err.contains("ForeignType"), "missing foreign type name: {err}");
}

#[test]
fn trait_orphan_rule_allows_local_trait_for_foreign_type() {
    let out = run(&[&example_path("proj_coherence_allowed/main.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
}

#[test]
fn explicit_generic_arguments_select_function_types() {
    let out = run(&["--run", &example_path("generics_explicit.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["7", "a", "Just(a)", "5 m", "1"]);
}

#[test]
fn explicit_generic_arguments_check_arity_types_and_bounds() {
    let out = run(&[&example_path("generics_explicit_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1042"), "missing explicit-generic diagnostic: {err}");
    assert!(err.contains("explicit generic argument"), "missing arity diagnostic: {err}");
    assert!(err.contains("Plain"), "missing explicit-bound type: {err}");
}

#[test]
fn generic_impls_generic_methods_and_constructors_are_preserved() {
    let out = run(&["--run", &example_path("generic_impls_and_methods.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "ok");
}

#[test]
fn generic_trait_arguments_specialize_method_signatures() {
    let out = run(&[&example_path("generic_trait_args_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1054"), "missing trait-signature diagnostic: {err}");
    assert!(err.contains("Bad.convert"), "missing implementation name: {err}");
}

#[test]
fn transitive_trait_defaults_dispatch_through_supertraits() {
    let out = run(&["--run", &example_path("traits_defaults_transitive.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "9");
}

#[test]
fn applied_generic_impls_dispatch_by_record_arguments() {
    let out = run(&["--run", &example_path("generic_impl_dispatch.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["integer", "text"]);
}

#[test]
fn applied_generic_impls_do_not_fall_back_to_another_type() {
    let out = run(&["--run", &example_path("generic_impl_dispatch_error.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1042"), "missing static method diagnostic: {err}");
    assert!(
        err.contains("Type 'Box<String>' has no method 'label'"),
        "missing applied type/method in diagnostic: {err}"
    );
}

#[test]
fn concrete_method_arguments_are_checked_before_runtime() {
    let out = run(&[&example_path("concrete_method_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1042"), "missing static argument diagnostic: {err}");
    assert!(
        err.contains("Argument for 'add' expects 'Int', got 'String'"),
        "missing method argument types in diagnostic: {err}"
    );
}

#[test]
fn trait_default_bodies_are_type_checked() {
    let out = run(&[&example_path("trait_default_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1041"), "missing default-body type diagnostic: {err}");
    assert!(
        err.contains("Broken.value") && err.contains("Int") && err.contains("String"),
        "missing default method and types in diagnostic: {err}"
    );
}

#[test]
fn impl_generic_bounds_are_honored_for_applied_methods() {
    let out = run(&[&example_path("impl_bounds_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1042"), "missing impl-bound method diagnostic: {err}");
    assert!(
        err.contains("Box<Plain>") && err.contains("no method 'label'"),
        "missing rejected applied type/method in diagnostic: {err}"
    );
}

#[test]
fn applied_generic_enum_impls_dispatch_by_constructor_arguments() {
    let out = run(&["--run", &example_path("generic_enum_dispatch.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["integer enum", "text enum"]);
}

#[test]
fn quantity_impls_dispatch_by_dimension_argument() {
    let out = run(&["--run", &example_path("quantity_impl_dispatch.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["length quantity", "time quantity"]);
}

#[test]
fn multi_file_project_resolves_qualified_alias_and_named_imports() {
    let out = run(&["--run", &example_path("proj1/main.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines, ["298.15", "273.15", "5"]);
}

#[test]
fn private_symbol_is_rejected_across_modules() {
    let out = run(&[&example_path("proj_private/main.ostrin")]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("E1080"));
}

#[test]
fn circular_import_is_detected() {
    let out = run(&[&example_path("proj_cycle/main.ostrin")]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("E1081"));
}

#[test]
fn parser_recovers_and_reports_every_syntax_error() {
    let out = run(&[&example_path("parse_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert_eq!(err.matches("parse error").count(), 3, "expected exactly 3 parse errors: {err}");
}

#[test]
fn path_dependency_resolves_and_runs() {
    let out = run(&["--run", &example_path("pkg_project/main_app/main.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "hola, Ostrin");
}

#[test]
fn git_dependency_fails_clearly_without_network_access() {
    let out = run(&[&example_path("pkg_git_test/main.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("does not fetch git dependencies automatically"), "unexpected message: {err}");
}
