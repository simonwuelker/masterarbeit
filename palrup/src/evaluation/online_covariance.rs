/// Computes covariance of data points in `O(1)` memory using welford.
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

    pub(crate) fn number_of_samples(&self) -> usize {
        self.number_of_samples
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

    pub(crate) fn combine(first: OnlineCovariance, second: OnlineCovariance) -> OnlineCovariance {
        let n1 = first.number_of_samples as f64;
        let n2 = second.number_of_samples as f64;
        let n = n1 + n2;

        // Handle empty accumulators
        if first.number_of_samples == 0 {
            return second;
        }
        if second.number_of_samples == 0 {
            return first;
        }

        let dx = second.average_x - first.average_x;
        let dy = second.average_y - first.average_y;
        let w = (n1 * n2) / n;

        OnlineCovariance {
            average_x: (n1 * first.average_x + n2 * second.average_x) / n,
            average_y: (n1 * first.average_y + n2 * second.average_y) / n,
            m2_x: first.m2_x + second.m2_x + dx * dx * w,
            m2_y: first.m2_y + second.m2_y + dy * dy * w,
            covariance: first.covariance + second.covariance + dx * dy * w,
            number_of_samples: first.number_of_samples + second.number_of_samples,
        }
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

    #[test]
    fn test_covariance_combine() {
        let mut first_covariance = OnlineCovariance::default();

        for (x, y) in iter::zip(&[1., 2., 4.], &[6., 0., 3.]) {
            first_covariance.add_sample(*x, *y);
        }

        let mut second_covariance = OnlineCovariance::default();
        for (x, y) in iter::zip(&[-1., 3., 0.], &[10., 1., 8.]) {
            second_covariance.add_sample(*x, *y);
        }

        let combined = OnlineCovariance::combine(first_covariance, second_covariance);

        assert_eq!(combined.population_covariance(), Some(-5.166666666666667));
        assert_eq!(combined.sample_covariance(), Some(-6.2));
    }
}
