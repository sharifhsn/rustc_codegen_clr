use serde::{Serialize,Deserialize};
#[derive(Serialize,Deserialize,Debug,PartialEq)] struct Inner{ id:u64, tags:Vec<String>, ratio:f64, active:bool }
#[derive(Serialize,Deserialize,Debug,PartialEq)] enum Kind{ A, B(i32), C{x:f64,y:f64} }
#[derive(Serialize,Deserialize,Debug,PartialEq)] struct Outer{ name:String, items:Vec<Inner>, kind:Kind, maybe:Option<i64> }
fn main(){
  let o=Outer{ name:"t".into(), items:vec![Inner{id:1,tags:vec!["a".into(),"b".into()],ratio:3.14,active:true}, Inner{id:2,tags:vec![],ratio:-0.5,active:false}], kind:Kind::C{x:1.0,y:2.0}, maybe:Some(42) };
  let j=serde_json::to_string(&o).unwrap(); println!("{}",j);
  let back:Outer=serde_json::from_str(&j).unwrap(); println!("rt={}", back==o);
  for k in [Kind::A, Kind::B(7), Kind::C{x:9.0,y:8.0}]{ println!("{}", serde_json::to_string(&k).unwrap()); }
  let v:serde_json::Value=serde_json::from_str(r#"{"a":[1,2,3],"b":{"c":true,"d":null},"e":1.5e10}"#).unwrap();
  println!("{}",v);
}
