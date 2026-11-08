//! SDK-only ordinary reader qualification package builder; no Host or native UI dependency.
use extension_api::{Manifest, SUPPORTED_HOST_APIS, HOST_API_V0_2, invocation::{InvocationRequest, INVOCATION_PROTOCOL}};
use qusor_package_probe::package::{build_signed_for_host, publisher_key, PackageFile};
use std::{error::Error, path::Path};
fn quoted(bytes:&[u8])->String {bytes.iter().map(|b|format!("\\{b:02x}")).collect()}
fn module(variant:&str)->Result<Vec<u8>,Box<dyn Error>> {
    let invocation=InvocationRequest {invocation_protocol:INVOCATION_PROTOCOL,api_version:HOST_API_V0_2,request_id:"own-invocation".into()}.to_json()?;
    let prefix1=b"{\"capture_protocol\":1,\"request_id\":\"surface-proposal\",\"draft\":";
    let prefix2=b"{\"capture_protocol\":2,\"request_id\":\"surface-proposal\",\"draft\":";
    let mut text=include_str!("reader.wat").to_owned();
    for (key,value) in [("@INVOCATION@",quoted(invocation.as_bytes())),("@INVOCATION_LEN@",invocation.len().to_string()),("@PREFIX1@",quoted(prefix1)),("@PREFIX2@",quoted(prefix2)),("@PREFIX_LEN@",prefix1.len().to_string()),("@DRAFT_MARKER@",quoted(b"\"draft_json\":\"")),("@DRAFT_MARKER_LEN@",b"\"draft_json\":\"".len().to_string()),("@PROTOCOL_MARKER@",quoted(b"\"capture_protocol\":")),("@PROTOCOL_MARKER_LEN@",b"\"capture_protocol\":".len().to_string())] {text=text.replace(key,&value);}
    if variant != "normal" {
        let call="(local.set $n (call $call (i32.const 4096) (local.get $n) (i32.const 8192) (i32.const 4096)))";
        let suffix=match variant { "trap" => "unreachable", "timeout" => "(loop $forever (br $forever))", "wrong" => "(return (i32.const 0) (i32.const 2))", "repeat" => "(drop (call $call (i32.const 4096) (local.get $context_len) (i32.const 8192) (i32.const 4096)))", _=> unreachable!() };
        text=text.replace("(local $n i32)","(local $n i32) (local $context_len i32)");
        text=text.replace(call,&format!("(local.set $context_len (local.get $n)) {call} {suffix}"));
    }
    Ok(wat::parse_str(&text)?)
}
fn main()->Result<(),Box<dyn Error>> {
    let args=std::env::args().skip(1).collect::<Vec<_>>();
    if args.len()!=3 {return Err("usage: build-surface-plugin MANIFEST SEED_HEX OUTPUT".into());}
    let manifest=Manifest::from_slice(&std::fs::read(&args[0])?)?;
    let seed_hex=&args[1];if seed_hex.len()!=64 {return Err("seed must be32 bytes".into());}
    let mut seed=[0;32];for (index,byte) in seed.iter_mut().enumerate(){*byte=u8::from_str_radix(&seed_hex[index*2..index*2+2],16)?;}
    let wasm=module("normal")?;
    let bytes=build_signed_for_host(manifest,"reader.wasm",vec![PackageFile{path:"reader.wasm".into(),mime:"application/wasm".into(),bytes:wasm}],&seed,&SUPPORTED_HOST_APIS)?;
    if let Some(parent)=Path::new(&args[2]).parent(){std::fs::create_dir_all(parent)?;}
    std::fs::write(&args[2],bytes)?;
    for variant in ["trap","timeout","wrong","repeat"] {
        let manifest=Manifest::from_slice(&std::fs::read(&args[0])?)?;
        let bytes=build_signed_for_host(manifest,"reader.wasm",vec![PackageFile{path:"reader.wasm".into(),mime:"application/wasm".into(),bytes:module(variant)?}],&seed,&SUPPORTED_HOST_APIS)?;
        std::fs::write(Path::new(&args[2]).with_extension(format!("{variant}.package")),bytes)?;
    }
    println!("{}",serde_json::json!({"publisher_key":publisher_key(&seed),"output":args[2]}));
    Ok(())
}
