#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct OnlineCovariance {
    average_x: f64,
    average_y: f64,
    covariance: f64,
    m2_x: f64,
    m2_y: f64,
    number_of_samples: usize,
}

impl OnlineCovariance {
    pub(crate) fn add_sample(&mut self, x: f64, y: f64) {
        self.number_of_samples += 1;
        let number_of_samples = self.number_of_samples as f64;

        let delta_x = x - self.average_x;
        self.average_x += delta_x / number_of_samples;

        let delta_y = y - self.average_y;
        self.average_y += delta_y / number_of_samples;

        let delta_x_2 = x - self.average_x;
        let delta_y_2 = y - self.average_y;

        self.m2_x += delta_x * delta_x_2;
        self.m2_y += delta_y * delta_y_2;

        self.covariance += delta_x * (y - self.average_y);
    }

    pub(crate) fn population_covariance(&self) -> Option<f64> {
        (self.number_of_samples > 0).then(|| self.covariance / self.number_of_samples as f64)
    }

    pub(crate) fn sample_covariance(&self) -> Option<f64> {
        (self.number_of_samples > 1)
            .then(|| self.covariance / (self.number_of_samples as f64 - 1.0))
    }

    pub(crate) fn pearson_correlation_coefficient(&self) -> Option<f64> {
        if self.number_of_samples == 0 {
            return None;
        }

        if self.m2_x <= 0.0 || self.m2_y <= 0.0 {
            return None;
        }

        Some(self.covariance / (self.m2_x * self.m2_y).sqrt())
    }
}

#[cfg(test)]
mod tests {
    use std::iter;

    use super::*;

    #[test]
    fn test_covariance_example() {
        let mut covariance = OnlineCovariance::default();

        for (x, y) in iter::zip(&[1., 2., 4.], &[6., 0., 3.]) {
            covariance.add_sample(*x, *y);
        }

        assert_eq!(covariance.population_covariance(), Some(-1.0));
        assert_eq!(covariance.sample_covariance(), Some(-1.5));
    }

    #[test]
    fn test_correlation_example() {
        let mut covariance = OnlineCovariance::default();

        for (x, y) in iter::zip(&[1., 2., 4.], &[6., 0., 3.]) {
            covariance.add_sample(*x, *y);
        }

        let expected = -0.327327;
        assert!(covariance.pearson_correlation_coefficient().unwrap() - expected < 0.001);
    }
}
