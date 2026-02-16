#![cfg(test)]
use serde_json::json;

use crate::args::Args;
use crate::bencher::{self, generate_result_json_str};
use crate::point_filters::PointFilterType;
use crate::runconfig::read_run_file;
use crate::utils::RunResults;
use crate::{
    args::RunArgs, filter::Filter, point_filters::PointFilter, runconfig::RunConfig, trace::Trace,
};
use crate::{run_runconfig, runconfig};
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::LazyLock;
use std::vec;

static V1_INPUT_PATH: LazyLock<PathBuf> = LazyLock::new(|| PathBuf::from("testdata/v1.ftrace"));
static V5_INPUT_PATH: LazyLock<PathBuf> = LazyLock::new(|| PathBuf::from("testdata/v5_1_1.ftrace"));
static V5_LCP_INPUT_PATH: LazyLock<PathBuf> =
    LazyLock::new(|| PathBuf::from("testdata/v5_1_1_LCP.ftrace"));
static V5_FCP_INPUT_PATH: LazyLock<PathBuf> =
    LazyLock::new(|| PathBuf::from("testdata/v5_1_1_FCP.ftrace"));

const V1_OUTPUT: &str = include_str!("../testdata/v1_output.json");
const V5_OUTPUT: &str = include_str!("../testdata/v5_1_1_output.json");
const V5_LCP_OUTPUT: &str = include_str!("../testdata/v5_1_1_LCP_output.json");
const V5_FCP_OUTPUT: &str = include_str!("../testdata/v5_1_1_FCP_output.json");

struct Testcase<'a> {
    input_file_path: PathBuf,
    output_file_str: &'a str,
}


#[test]
fn parse_pointfilter_json() -> anyhow::Result<()> {
    let runs_json_path = PathBuf::from_str("testdata/runs.json")?;
    let test_args = Args::test_default(runs_json_path.clone());
    let run_config: Vec<RunConfig> = read_run_file(&runs_json_path, &test_args)?;
    let run_config = &run_config[0];

    assert_eq!(run_config.run_args.url, "https://www.google.com");
    assert_eq!(run_config.run_args.tries, 5);
    assert_eq!(run_config.run_args.mitmproxy, true);


    assert_eq!(run_config.point_filters.len(), 4);
    let first = &run_config.point_filters[0];
    let snd = &run_config.point_filters[1];
    let third = &run_config.point_filters[2];
    let fourth = &run_config.point_filters[3];

    assert_eq!(first.match_str, "explicit");
    assert_eq!(first.name, "Explicit");
    assert_eq!(first.point_filter_type, PointFilterType::Default);

    assert_eq!(snd.match_str, "resident-according-to-smaps");
    assert_eq!(snd.name, "resident-smaps");
    assert_eq!(snd.point_filter_type, PointFilterType::Combined);

    assert_eq!(third.match_str, "LargestContentfulPaint");
    assert_eq!(third.name, "LargestContentfulPaint");
    assert_eq!(third.point_filter_type, PointFilterType::Largest);

    assert_eq!(fourth.match_str, "FirstContentfulPaint");
    assert_eq!(fourth.name, "FirstContentfulPaint");
    assert_eq!(fourth.point_filter_type, PointFilterType::Default);

    Ok(())
}

#[test]
fn full_default_v1() {
    parsing_common(Testcase {
        input_file_path: V1_INPUT_PATH.to_path_buf(),
        output_file_str: V1_OUTPUT,
    });
}

#[test]
fn full_default_v5() {
    parsing_common(Testcase {
        input_file_path: V5_INPUT_PATH.to_path_buf(),
        output_file_str: V5_OUTPUT,
    });
}

#[test]
fn full_v5_with_lcp() {
    parsing_common_with_extra_filters(
        Testcase {
            input_file_path: V5_LCP_INPUT_PATH.to_path_buf(),
            output_file_str: V5_LCP_OUTPUT,
        },
        vec![],
        vec![PointFilter {
            name: String::from("LargestContentfulPaint"),
            match_str: String::from("LargestContentfulPaint"),
            no_unit_conversion: true,
            point_filter_type: PointFilterType::Largest,
        }],
    );
}

#[test]
fn full_v5_with_fcp() {
    parsing_common_with_extra_filters(
        Testcase {
            input_file_path: V5_FCP_INPUT_PATH.to_path_buf(),
            output_file_str: V5_FCP_OUTPUT,
        },
        vec![],
        vec![PointFilter {
            name: String::from("FirstContentfulPaint"),
            match_str: String::from("FirstContentfulPaint"),
            no_unit_conversion: true,
            point_filter_type: PointFilterType::Default,
        }],
    );
}

#[test]
fn test_testcaseprofiling_v1_v5() {
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
            V1_INPUT_PATH.to_path_buf(),
            vec![],
            vec![PointFilter {
                name: String::from("TESTCASE_PROFILING"),
                match_str: String::from("generatehtml"),
                no_unit_conversion: true,
                point_filter_type: PointFilterType::Default,
            }]
        )
        .unwrap(),
        expected_json
    );
    assert_eq!(
        test_filters(
            V5_INPUT_PATH.to_path_buf(),
            vec![],
            vec![PointFilter {
                name: String::from("TESTCASE_PROFILING"),
                match_str: String::from("generatehtml"),
                no_unit_conversion: true,
                point_filter_type: PointFilterType::Default,
            }]
        )
        .unwrap(),
        expected_json
    );
}

#[test]
fn test_lcp_v5() {
    let point_filters = vec![PointFilter {
        name: String::from("LargestContentfulPaint"),
        match_str: String::from("LargestContentfulPaint"),
        no_unit_conversion: true,
        point_filter_type: PointFilterType::Largest,
    }];

    let expected_json = json!({
        "E2E/https://servo.org/LargestContentfulPaint/area": {
            "Pixels": {
            "value": 90810.0,
            "lower_value": 90810.0,
            "upper_value": 90810.0
            }
        },
        "E2E/https://servo.org/LargestContentfulPaint/paint_time": {
            "Nanoseconds": {
            "value": 231277380060022.0,
            "lower_value": 231277380060022.0,
            "upper_value": 231277380060022.0
            }
        }
    });

    assert_eq!(
        test_filters(V5_LCP_INPUT_PATH.to_path_buf(), vec![], point_filters).unwrap(),
        expected_json
    );
}

#[test]
fn test_fcp_v5() {
    // FirstContentfulPaint
    let point_filters = vec![PointFilter {
        name: String::from("FirstContentfulPaint"),
        match_str: String::from("FirstContentfulPaint"),
        no_unit_conversion: true,
        point_filter_type: PointFilterType::Default,
    }];

    let expected_json = json!({
        "E2E/https://servo.org/FirstContentfulPaint/paint_time": {
            "Nanoseconds": {
            "value": 271633800350218.0,
            "lower_value": 271633800350218.0,
            "upper_value": 271633800350218.0
            }
        }
    });

    assert_eq!(
        test_filters(V5_FCP_INPUT_PATH.to_path_buf(), vec![], point_filters).unwrap(),
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
    parsing_common_with_extra_filters(testcase, vec![], vec![]);
}

fn parsing_common_with_extra_filters(
    testcase: Testcase,
    extra_filter: Vec<Filter>,
    extra_point_filters: Vec<PointFilter>,
) {
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
            point_filter_type: PointFilterType::Default,
        },
        PointFilter::new(String::from("Resident"), String::from("resident")),
        PointFilter::new(String::from("LayoutThread"), String::from("layout-thread")),
        PointFilter::new(String::from("image-cache"), String::from("image-cache")),
        PointFilter::new(String::from("JS"), String::from("js")),
        PointFilter {
            name: String::from("resident-smaps"),
            match_str: String::from("resident-according-to-smaps"),
            no_unit_conversion: false,
            point_filter_type: PointFilterType::Combined,
        },
    ];

    let json_result = test_filters(
        input,
        filters.into_iter().chain(extra_filter).collect(),
        point_filters
            .into_iter()
            .chain(extra_point_filters)
            .collect(),
    )
    .unwrap();
    let expectex_json_result: serde_json::Value =
        serde_json::from_str(output).expect("Could not parse json");
    assert_eq!(json_result, expectex_json_result);
}

#[test]
fn test_run_with_old_json() {
    let runs_output = include_str!("../testdata/runs_output.json");
    let args = Args::test_default(V5_LCP_INPUT_PATH.to_path_buf());
    let run_configs = runconfig::read_run_file(&PathBuf::from("runs.json"), &args).unwrap();

    let all_bencher = run_configs.iter().all(|r| r.args.bencher);
    let all_print = run_configs.iter().all(|r| !r.args.bencher);
    if !all_bencher && !all_print {
        panic!("We only support all bencher or all print runs");
    }
    let be_loud_filter = if args.quiet || all_bencher {
        log::LevelFilter::Error
    } else {
        log::LevelFilter::Info
    };

    env_logger::builder().filter_level(be_loud_filter).init();

    let mut filter_results = HashMap::new();
    let mut filter_errors = HashMap::new();
    let mut point_results = HashMap::new();
    for run_config in run_configs {
        run_runconfig(
            &run_config,
            &mut filter_results,
            &mut filter_errors,
            &mut point_results,
        )
        .unwrap();
    }

    let result = bencher::generate_result_json_str(RunResults {
        prepend: args.prepend.clone(),
        filter_results,
        errors: filter_errors,
        point_results,
    })
    .unwrap();

    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&result).unwrap(),
        serde_json::from_str::<serde_json::Value>(runs_output).expect("Could not parse json")
    )
}
