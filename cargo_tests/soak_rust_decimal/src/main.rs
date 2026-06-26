use rust_decimal::Decimal; use std::str::FromStr;
fn main(){
  let a=Decimal::from_str("12345.6789").unwrap(); let b=Decimal::from_str("0.0013").unwrap();
  println!("add={} sub={} mul={} div={}",a+b,a-b,a*b,a/b);
  let mut s=Decimal::ZERO; for i in 1..=2000i64{ s += Decimal::from(i)/Decimal::from(7); } println!("sum={}",s);
  let big=Decimal::from_str("79228162514264337593543950335").unwrap();
  println!("big={} half={}",big,big/Decimal::from(2));
  println!("round={}", Decimal::from_str("2.5").unwrap().round_dp(0));
  let neg=Decimal::from_str("-9999.99999").unwrap(); println!("negsq={}", neg*neg);
  let mut p=Decimal::ONE; for _ in 0..20{ p*=Decimal::from_str("1.05").unwrap(); } println!("pow={}",p);
}
