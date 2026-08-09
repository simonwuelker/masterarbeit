use tabled::{builder::Builder, settings::Style};

use super::online_covariance::OnlineCovariance;

use std::iter;

pub(crate) const NUMBER_OF_METRICS: usize = 7;

#[derive(Clone, Copy, Debug)]
pub(crate) struct MetricSet {
    pub is_critical: bool,
    pub incoming_edges: usize,
    pub outgoing_edges: usize,
    pub number_of_literals: usize,
    pub id: usize,
    pub lifetime: usize,
    pub minimum_lifetime: usize,
}

pub(crate) fn metric_name_for(index: usize) -> &'static str {
    match index {
        0 => "is critical",
        1 => "# incoming edges",
        2 => "# outgoing edges",
        3 => "# literals",
        4 => "Clause ID",
        5 => "Lifetime",
        6 => "Minimum lifetime",
        _ => unreachable!("Metric index out of bounds"),
    }
}

impl MetricSet {
    fn get_metric_at_index(&self, index: usize) -> f64 {
        match index {
            0 => self.is_critical as u8 as f64,
            1 => self.incoming_edges as f64,
            2 => self.outgoing_edges as f64,
            3 => self.number_of_literals as f64,
            4 => self.id as f64,
            5 => self.lifetime as f64,
            6 => self.minimum_lifetime as f64,
            _ => unreachable!("Metric index out of bounds"),
        }
    }
}

#[derive(Clone)]
pub struct CovarianceSet {
    covariances: [Box<[OnlineCovariance]>; NUMBER_OF_METRICS],
}

impl Default for CovarianceSet {
    fn default() -> Self {
        Self {
            covariances: (0..NUMBER_OF_METRICS)
                .map(|index| {
                    vec![OnlineCovariance::default(); NUMBER_OF_METRICS - index].into_boxed_slice()
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
        }
    }
}

impl CovarianceSet {
    pub(crate) fn add_sample(&mut self, metrics: MetricSet) {
        for i in 0..NUMBER_OF_METRICS {
            for j in i..NUMBER_OF_METRICS {
                self.covariance_at_mut(i, j).add_sample(
                    metrics.get_metric_at_index(i),
                    metrics.get_metric_at_index(j),
                );
            }
        }
    }

    fn covariance_at(&self, i: usize, j: usize) -> &OnlineCovariance {
        &self.covariances[i][j - i]
    }

    fn covariance_at_mut(&mut self, i: usize, j: usize) -> &mut OnlineCovariance {
        &mut self.covariances[i][j - i]
    }

    pub(crate) fn population_covariance(&self) -> Option<[Box<[f64]>; NUMBER_OF_METRICS]> {
        (0..NUMBER_OF_METRICS)
            .map(|i| {
                (i..NUMBER_OF_METRICS)
                    .map(|j| self.covariance_at(i, j).population_covariance())
                    .collect::<Option<Box<_>>>()
            })
            .collect::<Option<Vec<_>>>()
            .map(|values| values.try_into().unwrap())
    }

    pub(crate) fn sample_covariance(&self) -> Option<[Box<[f64]>; NUMBER_OF_METRICS]> {
        (0..NUMBER_OF_METRICS)
            .map(|i| {
                (i..NUMBER_OF_METRICS)
                    .map(|j| self.covariance_at(i, j).sample_covariance())
                    .collect::<Option<Box<_>>>()
            })
            .collect::<Option<Vec<_>>>()
            .map(|values| values.try_into().unwrap())
    }

    pub(crate) fn pearson_correlation(&self) -> Option<PearsonCorrelation> {
        (0..NUMBER_OF_METRICS)
            .map(|i| {
                (i..NUMBER_OF_METRICS)
                    .map(|j| self.covariance_at(i, j).pearson_correlation_coefficient())
                    .collect::<Option<Box<_>>>()
            })
            .collect::<Option<Vec<_>>>()
            .map(|values| values.try_into().unwrap())
            .map(|values| PearsonCorrelation { values })
    }

    pub(crate) fn combine(first: Self, second: Self) -> Self {
        Self {
            covariances: first
                .covariances
                .iter()
                .zip(second.covariances.iter())
                .map(|(first_row, second_row)| {
                    first_row
                        .iter()
                        .zip(second_row.iter())
                        .map(|(first_value, second_value)| {
                            OnlineCovariance::combine(*first_value, *second_value)
                        })
                        .collect::<Box<[OnlineCovariance]>>()
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
        }
    }
}

pub(crate) struct PearsonCorrelation {
    values: [Box<[f64]>; NUMBER_OF_METRICS],
}

impl PearsonCorrelation {
    pub(crate) fn debug_print(&self) {
        let mut table_builder =
            Builder::with_capacity(NUMBER_OF_METRICS + 1, NUMBER_OF_METRICS + 1);
        table_builder.push_record(
            iter::once("Pearson").chain(
                (0..NUMBER_OF_METRICS)
                    .map(|index| metric_name_for(index))
                    .collect::<Vec<_>>(),
            ),
        );
        for row in 0..NUMBER_OF_METRICS {
            let mut row_data = Vec::with_capacity(NUMBER_OF_METRICS + 1);
            row_data.push(metric_name_for(row).to_string());
            for column in 0..NUMBER_OF_METRICS {
                if row > column {
                    row_data.push("".to_string());
                    continue;
                }

                row_data.push(format!("{:.5}", self.values[row][column - row]));
            }

            table_builder.push_record(row_data);
        }

        let mut table = table_builder.build();
        table.with(Style::modern());
        println!("{table}");
    }
}
