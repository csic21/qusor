"""Build and sign a component using only a freshly exported SDK outside the repository."""
from pathlib import Path
import json, subprocess, sys, tempfile, os

def main():
    root=Path(__file__).resolve().parents[2];sys.path.insert(0,str(root/'experiments/capture-core'));from export_sdk import export_sdk
    outside=Path(tempfile.mkdtemp(prefix='qusor-component-public-author-')); sdk=outside/'sdk'; hashes=export_sdk(sdk)
    author=outside/'component-author';(author/'src').mkdir(parents=True)
    (author/'Cargo.toml').write_text('[workspace]\nresolver="3"\n[package]\nname="ordinary-component-author"\nversion="0.1.0"\nedition="2024"\n[dependencies]\nextension-api={path="../sdk/api"}\nextension-package={path="../sdk/package"}\n')
    code='''use extension_api::{Manifest, HOST_API, HOST_API_V0_3, composition::CompositionDescriptor, draft::DraftRequest};
    use extension_package::package::{PackageFile,build_signed_for_host,verify_for_host,publisher_key};
    fn main(){
     let manifest=Manifest::from_json(include_str!("manifest.json")).unwrap();
     assert!(!manifest.supports_api(HOST_API));assert!(manifest.supports_api(HOST_API_V0_3));
     let asset=include_bytes!("composition.json");let descriptor=CompositionDescriptor::from_slice(asset).unwrap();descriptor.validate_for_manifest(&manifest).unwrap();
     let request=DraftRequest::from_json(include_str!("request.json")).unwrap();assert_eq!(DraftRequest::from_json(&request.to_json().unwrap()).unwrap(),request);
     let package=build_signed_for_host(manifest,"module.wasm",vec![PackageFile{path:"module.wasm".into(),mime:"application/wasm".into(),bytes:b"\\0asm\\x01\\0\\0\\0".to_vec()},PackageFile{path:"assets/ui-composition.json".into(),mime:"application/json".into(),bytes:asset.to_vec()}],&[64;32],&[HOST_API_V0_3]).unwrap();
     let verified=verify_for_host(&package,&publisher_key(&[64;32]),&[HOST_API_V0_3]).unwrap();assert!(verified.file("assets/ui-composition.json").is_some());
     assert!(verify_for_host(&package,&publisher_key(&[64;32]),&[HOST_API]).is_err());
     println!("public SDK0.3 independent author and signed composition verified");
    }'''
    (author/'src/main.rs').write_text(code)
    manifest={'manifest_version':1,'package_id':'ordinary.external-editor','publisher_id':'ordinary.author','package_version':{'major':1,'minor':0,'patch':0},'host_api':{'major':0,'min_minor':3,'max_minor':3},'required_capabilities':['note.draft.edit'],'optional_capabilities':[],'contributions':[{'id':'ordinary.external-editor.input','title':'Thought','kind':'component_provider','required_capabilities':['note.draft.edit']}]}
    asset={'composition_protocol':1,'api_version':{'major':0,'minor':3},'contribution_id':'ordinary.external-editor.input','role':'annotation.plain-text-editor','contract':'annotation.plain-text-editor/v1','view':{'kind':'text_editor','label':'My thought','placeholder':'Write'}}
    request={'api_version':{'major':0,'minor':3},'draft_protocol':1,'request_id':'read','resource':{'kind':'draft','vault_id':'vault','draft_id':'1'*32},'mount_id':'2'*32,'operation':{'kind':'read_chunk','revision':None,'offset':0,'max_bytes':512}}
    for name,data in [('manifest',manifest),('composition',asset),('request',request)]: (author/f'src/{name}.json').write_text(json.dumps(data))
    env=os.environ.copy();env['CARGO_NET_OFFLINE']='true';env['RUSTUP_TOOLCHAIN']='1.97.1';records=[]
    for args in [['cargo','generate-lockfile','--offline'],['cargo','run','--locked','--offline']]:
     r=subprocess.run(args,cwd=author,env=env,text=True,capture_output=True);records.append({'command':args,'exit':r.returncode,'stdout':r.stdout,'stderr':r.stderr});assert r.returncode==0,records[-1]
    evidence=root/'experiments/capture-core/evidence/linux-component-sdk-author.json';evidence.write_text(json.dumps({'ok':True,'outside_repository':str(outside),'public_sdk_fingerprints':hashes,'author_sources':{p.name:p.read_text() for p in sorted((author/'src').iterdir())},'commands':records,'scope':'Independent native author compile/sign/verify of 0.3 public contracts; no plugin execution or native replacement'},ensure_ascii=False,indent=2)+'\n');print(evidence.relative_to(root))

if __name__ == "__main__":
    main()
