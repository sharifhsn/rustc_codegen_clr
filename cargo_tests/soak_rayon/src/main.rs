use rayon::prelude::*;
fn main(){
  let v:Vec<i64>=(0..1_000_000).collect();
  println!("sum={} max={:?}", v.par_iter().sum::<i64>(), v.par_iter().max());
  println!("sqsum={}", v.par_iter().map(|x|x%1000).map(|x|x*x).sum::<i64>());
  let mut s:Vec<i64>=(0..100000).map(|x|(x*7919)%100000).collect();
  s.par_sort();
  println!("sorted={} first={} last={}", s.windows(2).all(|w|w[0]<=w[1]), s[0], s[s.len()-1]);
  println!("fb={}", (0..1_000_000i64).into_par_iter().filter(|x|x%3==0||x%5==0).count());
}
