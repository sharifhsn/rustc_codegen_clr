use itertools::Itertools;
fn main(){
  println!("{:?}", (1..=4).permutations(2).collect_vec());
  println!("{:?}", (1..=5).combinations(3).collect_vec());
  println!("{:?}", (1..=10).chunks(3).into_iter().map(|c|c.sum::<i32>()).collect_vec());
  println!("{:?}", vec![1,1,2,3,3,3,4].into_iter().dedup_with_count().collect_vec());
  println!("{:?}", (1..=6).cartesian_product('a'..='b').collect_vec());
  println!("{}", (1..=100).map(|x|x*x).sum::<i64>());
  println!("{:?}", "the quick brown fox".split(' ').sorted().collect_vec());
}
