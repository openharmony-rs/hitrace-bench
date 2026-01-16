#![cfg(test)]
use serde_json::json;

use crate::args::Args;
use crate::bencher::generate_result_json_str;
use crate::run_runconfig;
use crate::utils::RunResults;
use crate::{
    args::RunArgs, filter::Filter, point_filters::PointFilter, runconfig::RunConfig, trace::Trace,
};
use std::collections::HashMap;
use std::path::PathBuf;

const V1_INPUT_PATH_STR: &str = "testdata/v1.ftrace";
const V5_INPUT_PATH_STR: &str = "testdata/v5_1_1.ftrace";

struct Testcase<'a> {
    input_file_path: PathBuf,
    output_file_str: &'a str,
}

#[test]
fn parsing_v1() {
    parsing_common(Testcase {
        input_file_path: PathBuf::from("testdata/v1.ftrace"),
        output_file_str: include_str!("../testdata/v1_output.json"),
    });
}

#[test]
fn parsing_v5() {
    parsing_common(Testcase {
        input_file_path: PathBuf::from("testdata/v5_1_1.ftrace"),
        output_file_str: include_str!("../testdata/v5_1_1_output.json"),
    });
}

#[test]
fn test_testcase_regex() {
    let point_filters = vec![PointFilter {
        name: String::from("TESTCASE_PROFILING"),
        match_str: String::from("generatehtml"),
        no_unit_conversion: true,
        combined: false,
    }];

    let expected_json = json!({
        "E2E/https://servo.org/": {
            "Data": {
                "lower_value": 1720.0,
                "upper_value": 1720.0,
                "value": 1720.0
            }
        }
    });

    assert_eq!(
        test_filters(
            PathBuf::from(V1_INPUT_PATH_STR),
            vec![],
            point_filters.clone()
        )
        .unwrap(),
        expected_json
    );
    assert_eq!(
        test_filters(
            PathBuf::from(V5_INPUT_PATH_STR),
            vec![],
            point_filters.clone()
        )
        .unwrap(),
        expected_json
    );
}

fn test_filters(
    input_file: PathBuf,
    filter: Vec<Filter>,
    point_filters: Vec<PointFilter>,
) -> Option<serde_json::Value> {
    let args = Args::test_default(input_file);

    let mut filter_results = HashMap::new();
    let mut filter_errors = HashMap::new();
    let mut point_results = HashMap::new();

    run_runconfig(
        &RunConfig::new(args.clone(), RunArgs::default(), filter, point_filters),
        &mut filter_results,
        &mut filter_errors,
        &mut point_results,
    )
    .expect("Could not create run_config");

    let run_results = RunResults {
        prepend: args.prepend.clone(),
        filter_results,
        errors: filter_errors,
        point_results,
    };

    Some(
        serde_json::from_str(
            &generate_result_json_str(run_results).expect("Error generating json result"),
        )
        .unwrap(),
    )
}

fn parsing_common(testcase: Testcase) {
    let (input, output) = (testcase.input_file_path, testcase.output_file_str);

    let filters = vec![
        Filter {
            name: String::from("Surface->LoadStart"),
            first: Box::new(|t: &Trace| t.function.contains("on_surface_created_cb")),
            last: Box::new(|t: &Trace| t.function.contains("load status changed Head")),
        },
        Filter {
            name: String::from("Load->Compl"),
            first: Box::new(|t: &Trace| t.function.contains("load status changed Head")),
            last: Box::new(|t: &Trace| t.function.contains("PageLoadEndedPrompt")),
        },
    ];
    let point_filters = vec![
        PointFilter {
            name: String::from("Explicit"),
            match_str: String::from("explicit"),
            no_unit_conversion: false,
            combined: false,
        },
        PointFilter::new(String::from("Resident"), String::from("resident")),
        PointFilter::new(String::from("LayoutThread"), String::from("layout-thread")),
        PointFilter::new(String::from("image-cache"), String::from("image-cache")),
        PointFilter::new(String::from("JS"), String::from("js")),
        PointFilter {
            name: String::from("resident-smaps"),
            match_str: String::from("resident-according-to-smaps"),
            no_unit_conversion: false,
            combined: true,
        },
    ];

    let json_result = test_filters(input, filters, point_filters).unwrap();
    let expectex_json_result: serde_json::Value =
        serde_json::from_str(output).expect("Could not parse json");
    assert_eq!(json_result, expectex_json_result);
}
