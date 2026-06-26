fn fib(n:u64)->u64{ if n<2{n}else{fib(n-1)+fib(n-2)} }
fn ack(m:u64,n:u64)->u64{ if m==0{n+1}else if n==0{ack(m-1,1)}else{ack(m-1,ack(m,n-1))} }
fn main(){
  println!("fib32={} ack(3,3)={}", fib(32), ack(3,3));
  let n=200000usize; let mut s=vec![true;n]; let mut c=0u64;
  for i in 2..n{ if s[i]{ c+=1; let mut j=i*i; while j<n{ s[j]=false; j+=i; } } } println!("primes={}",c);
  let mut chk=0u64;
  for py in 0..60{ for px in 0..60{
    let (x0,y0)=(px as f64/30.0-2.0, py as f64/30.0-1.0);
    let (mut x,mut y,mut it)=(0.0f64,0.0f64,0u32);
    while x*x+y*y<=4.0 && it<1000{ let xt=x*x-y*y+x0; y=2.0*x*y+y0; x=xt; it+=1; }
    chk=chk.wrapping_mul(31).wrapping_add(it as u64);
  }} println!("mandel={}",chk);
  let mut v:Vec<i64>=(0..20000).map(|x|(x*1103515245i64+12345)%100000).collect();
  v.sort_unstable(); println!("sorted={} sum={}", v.windows(2).all(|w|w[0]<=w[1]), v.iter().sum::<i64>());
  let words="the cat sat on the mat the cat ran";
  use std::collections::HashMap; let mut m:HashMap<&str,u32>=HashMap::new();
  for w in words.split(' '){ *m.entry(w).or_insert(0)+=1; }
  let mut kv:Vec<_>=m.into_iter().collect(); kv.sort(); println!("wc={:?}",kv);
}
