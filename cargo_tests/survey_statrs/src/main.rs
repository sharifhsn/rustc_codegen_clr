// survey_statrs — exercises statrs::distribution Normal/Binomial at fixed points.
//
// DETERMINISM: every line is `label = value` with floats printed at {:.6}; the
// inputs are hard-coded constants. No RNG, no time, no maps. The distribution
// constructors return Result, so we match them and print a marker rather than
// unwrapping (no panic path).

use statrs::distribution::{Binomial, Continuous, ContinuousCDF, Discrete, DiscreteCDF, Normal};
use statrs::statistics::{Distribution, Max, Min};

fn main() {
    // ---- Normal(mean=2.0, std_dev=3.0) -----------------------------------
    match Normal::new(2.0, 3.0) {
        Ok(n) => {
            // pdf / cdf at a few fixed abscissae.
            println!("normal_pdf_at_2.0 = {:.6}", n.pdf(2.0));
            println!("normal_pdf_at_0.0 = {:.6}", n.pdf(0.0));
            println!("normal_pdf_at_5.0 = {:.6}", n.pdf(5.0));
            println!("normal_cdf_at_2.0 = {:.6}", n.cdf(2.0));
            println!("normal_cdf_at_0.0 = {:.6}", n.cdf(0.0));
            println!("normal_cdf_at_5.0 = {:.6}", n.cdf(5.0));
            println!("normal_sf_at_5.0  = {:.6}", n.sf(5.0));
            // inverse cdf (quantile) at a fixed probability.
            println!("normal_invcdf_0.975 = {:.6}", n.inverse_cdf(0.975));
            // moments — Distribution::mean/variance/std_dev return Option.
            match n.mean() {
                Some(m) => println!("normal_mean = {:.6}", m),
                None => println!("normal_mean = <none>"),
            }
            match n.variance() {
                Some(v) => println!("normal_variance = {:.6}", v),
                None => println!("normal_variance = <none>"),
            }
            match n.std_dev() {
                Some(s) => println!("normal_std_dev = {:.6}", s),
                None => println!("normal_std_dev = <none>"),
            }
            match n.entropy() {
                Some(e) => println!("normal_entropy = {:.6}", e),
                None => println!("normal_entropy = <none>"),
            }
        }
        Err(_) => println!("normal_construct = <err>"),
    }

    // ---- Standard Normal(0,1): a couple of canonical reference points -----
    match Normal::new(0.0, 1.0) {
        Ok(z) => {
            println!("stdnorm_cdf_at_0.0 = {:.6}", z.cdf(0.0));   // 0.5
            println!("stdnorm_cdf_at_1.0 = {:.6}", z.cdf(1.0));   // ~0.841345
            println!("stdnorm_cdf_at_-1.0 = {:.6}", z.cdf(-1.0)); // ~0.158655
            println!("stdnorm_pdf_at_0.0 = {:.6}", z.pdf(0.0));   // ~0.398942
        }
        Err(_) => println!("stdnorm_construct = <err>"),
    }

    // ---- Binomial(p=0.3, n=10) -------------------------------------------
    match Binomial::new(0.3, 10) {
        Ok(b) => {
            // pmf at several fixed k.
            println!("binom_pmf_k0 = {:.6}", b.pmf(0));
            println!("binom_pmf_k3 = {:.6}", b.pmf(3));
            println!("binom_pmf_k5 = {:.6}", b.pmf(5));
            println!("binom_pmf_k10 = {:.6}", b.pmf(10));
            // cdf at fixed k.
            println!("binom_cdf_k3 = {:.6}", b.cdf(3));
            println!("binom_cdf_k5 = {:.6}", b.cdf(5));
            println!("binom_sf_k5  = {:.6}", b.sf(5));
            // support bounds (integer-valued).
            println!("binom_min = {}", b.min());
            println!("binom_max = {}", b.max());
            // moments.
            match b.mean() {
                Some(m) => println!("binom_mean = {:.6}", m),
                None => println!("binom_mean = <none>"),
            }
            match b.variance() {
                Some(v) => println!("binom_variance = {:.6}", v),
                None => println!("binom_variance = <none>"),
            }
        }
        Err(_) => println!("binom_construct = <err>"),
    }

    println!("== survey_statrs done ==");
}
