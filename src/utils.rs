use std::{collections::HashMap, iter::Sum};

use thiserror::Error;
use time::Duration;

use crate::{point_filters::PointFilter, trace::Trace};

pub(crate) struct AvgMingMax<T> {
    pub(crate) avg: T,
    pub(crate) min: T,
    pub(crate) max: T,
    /// Please don't do more than `u16` runs.
    pub(crate) number: u16,
}

pub(crate) fn avg_min_max<T, U>(values: &[T]) -> AvgMingMax<T>
where
    T: Ord + Sum<T> + Copy + std::ops::Div<U, Output = T>,
    U: TryFrom<usize> + From<u16> + Copy,
{
    let number: u16 = values.len().try_into().expect("You have too many runs");
    let min: T = *values.iter().min().expect("Could not find min");
    let max: T = *values.iter().max().expect("Could not find max");
    let sum: T = values.iter().cloned().sum();
    let avg = sum / number.into();
    AvgMingMax {
        avg,
        min,
        max,
        number,
    }
}

pub(crate) type FilterResults = HashMap<String, Vec<Duration>>;
pub(crate) type FilterErrors = HashMap<String, u32>;
pub(crate) type PointResults = HashMap<String, PointResult>;

#[derive(Error, Debug)]
pub(crate) enum PointError {
    #[error(
        "Too many traces are matching this pointfilter ({point_filter:?}) and combined not selected {traces:?}"
    )]
    TooManyTracesMatching {
        point_filter: PointFilter,
        traces: Vec<Trace>,
    },
}

#[derive(Debug)]
pub(crate) struct PointResult {
    pub(crate) no_unit_conversion: bool,
    pub(crate) result: Vec<u64>,
}

/// The results of a run given by filter.name, Vec<duration>
/// Notice that not all vectors will have the same length as some runs might fail.
#[derive(Debug)]
pub(crate) struct RunResults {
    /// Filter results
    pub(crate) filter_results: FilterResults,
    pub(crate) errors: FilterErrors,
    pub(crate) point_results: PointResults,
}
