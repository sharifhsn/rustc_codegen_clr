use dashmap::DashMap;
fn main(){
  let m=DashMap::new();
  for i in 0..10000i64{ m.insert(i%1000, i); }
  println!("len={}", m.len());
  let s:i64=(0..1000i64).filter_map(|k| m.get(&k).map(|v|*v)).sum(); println!("sum={}",s);
  m.alter(&5,|_,v|v*2); println!("k5={:?}", m.get(&5).map(|v|*v));
  m.remove(&7); println!("has7={}", m.contains_key(&7));
}
