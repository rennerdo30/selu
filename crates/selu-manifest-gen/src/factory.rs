//! Bounded abstract interpretation of source constructors; never runs Lua.
//! Local bindings and closure environments are lexical. Unresolved operations stay untyped.
use full_moon::{ast::*, node::Node};
use selu_manifest::{Declaration, Manifest};
use std::{collections::BTreeMap, cell::RefCell, rc::Rc};
#[derive(Clone, Default)]
enum Value {
    #[default] Unknown,
    Nil, Symbol(String), Object(Rc<RefCell<Object>>), Function(usize),
    Strings(Vec<String>), Widget(String), Number, Bool, Iterator(Vec<String>),
}
#[derive(Clone, Default)]
struct Object { members: BTreeMap<String, Value>, signatures: BTreeMap<String, Declaration>, proxy: Option<Value>, element: Option<String> }
#[derive(Clone)]
struct Function { body: FunctionBody, scope: usize, signature: Declaration, method: bool }
struct Scope { parent: Option<usize>, values: BTreeMap<String, Value> }
struct Analysis { scopes: Vec<Scope>, functions: Vec<Function>, exports: BTreeMap<String, Value>, fuel: usize, file: String }
impl Analysis {
    fn scope(&mut self, parent: usize) -> usize { let id=self.scopes.len(); self.scopes.push(Scope {parent:Some(parent),values:BTreeMap::new()});id }
    fn get(&self, scope: usize, name: &str) -> Value {
        self.scopes[scope].values.get(name).cloned().unwrap_or_else(|| self.scopes[scope].parent.map(|p|self.get(p,name)).unwrap_or_else(||Value::Symbol(name.into())))
    }
    fn assign(&mut self, scope: usize, name: &str, value: Value) {
        if self.scopes[scope].values.contains_key(name) || scope==0 { self.scopes[scope].values.insert(name.into(), value); }
        else { self.assign(self.scopes[scope].parent.unwrap_or(0),name,value); }
    }
    fn member(&self, object: Value, key: &str) -> Value { match object {
        Value::Object(o)=>o.borrow().members.get(key).cloned().unwrap_or_default(),
        Value::Symbol(s)=>Value::Symbol(format!("{s}.{key}")), _=>Value::Unknown,
    }}
    fn function(&mut self, body: &FunctionBody, scope: usize, name: &str, method: bool) -> Value {
        let id=self.functions.len();
        let mut signature=super::source::source_signature(name,body,"",&self.file);
        if !method && signature.arguments.first().is_some_and(|a|a.name=="self") { signature.arguments.remove(0); }
        self.functions.push(Function {body:body.clone(),scope,signature,method});Value::Function(id)
    }
    fn expr(&mut self, e: &Expression, scope: usize, depth: usize) -> Value {
        if depth>24 || self.fuel==0 { return Value::Unknown; } self.fuel-=1;
        match e {
            Expression::Var(Var::Name(t))=>self.get(scope,t.to_string().trim()),
            Expression::Var(Var::Expression(v))=>self.suffix(v.prefix(),v.suffixes(),scope,depth),
            Expression::FunctionCall(c)=>self.suffix(c.prefix(),c.suffixes(),scope,depth),
            Expression::Function(f)=>self.function(f.body(),scope,"callback",false),
            Expression::TableConstructor(t)=> {
                let mut object=Object::default();let mut strings=vec![];
                for field in t.fields() { match field {
                    Field::NameKey{key,value,..}=>{let v=self.expr(value,scope,depth+1);object.members.insert(key.to_string().trim().into(),v);},
                    Field::NoKey(v)=>if let Some(s)=super::source::literal(v).and_then(|v|v.as_str().map(str::to_owned)){strings.push(s)}, _=>{}
                }}
                if strings.is_empty(){Value::Object(Rc::new(RefCell::new(object)))}else{Value::Strings(strings)}
            }
            Expression::Number(_)=>Value::Number,
            Expression::Symbol(t) if t.to_string().trim()=="nil"=>Value::Nil,
            Expression::Symbol(t) if ["true","false"].contains(&t.to_string().trim())=>Value::Bool,
            Expression::Parentheses{expression,..}=>self.expr(expression,scope,depth+1),
            Expression::BinaryOperator{lhs,binop,rhs} if binop.to_string().trim()=="or"=> {
                let a=self.expr(lhs,scope,depth+1);if matches!(a,Value::Unknown|Value::Nil){self.expr(rhs,scope,depth+1)}else{a}
            }
            Expression::BinaryOperator{binop,..} if ["==","~=","<",">","<=",">="].contains(&binop.to_string().trim())=>Value::Bool,
            _=>Value::Unknown,
        }
    }
    fn suffix<'a>(&mut self, prefix: &Prefix, suffixes: impl Iterator<Item=&'a Suffix>, scope: usize, depth:usize)->Value {
        let mut v=match prefix {Prefix::Name(t)=>self.get(scope,t.to_string().trim()),Prefix::Expression(e)=>self.expr(e,scope,depth+1),_=>Value::Unknown};
        for suffix in suffixes {v=match suffix {
            Suffix::Index(Index::Dot{name,..})=>self.member(v,name.to_string().trim()),
            Suffix::Call(Call::AnonymousCall(args))=>{let args=self.args(args,scope,depth);self.call(v,args,depth+1)},
            Suffix::Call(Call::MethodCall(m))=>{let args=self.args(m.args(),scope,depth);self.method(v,m.name().to_string().trim(),args,depth+1)},
            _=>Value::Unknown,
        }} v
    }
    fn args(&mut self, args:&FunctionArgs,scope:usize,depth:usize)->Vec<Value>{match args {
        FunctionArgs::Parentheses{arguments,..}=>arguments.iter().map(|e|self.expr(e,scope,depth+1)).collect(),_=>vec![]
    }}
    fn call(&mut self, f:Value,args:Vec<Value>,depth:usize)->Value {
        if depth>24 || self.fuel==0{return Value::Unknown;}
        match f {
            Value::Function(id)=>{
                let f=self.functions[id].clone();let scope=self.scope(f.scope);
                for (i,p) in f.body.parameters().iter().enumerate(){self.scopes[scope].values.insert(p.to_string().trim().into(),args.get(i).cloned().unwrap_or_default());}
                self.block(f.body.block(),scope,depth+1).unwrap_or_default()
            }
            Value::Symbol(name)=>match name.as_str(){
                "CreateFromMixins"=>{
                    let mut obj=Object::default(); for a in args {if let Value::Object(a)=a{let a=a.borrow();obj.members.extend(a.members.clone());obj.signatures.extend(a.signatures.clone());}}
                    Value::Object(Rc::new(RefCell::new(obj)))
                }
                "ProxyUtil.CreateProxyMixin"=>{
                    let mut obj=Object::default();if let (Some(Value::Object(private)),Some(Value::Strings(names)))=(args.get(1),args.get(2)) {
                        for n in names{let p=private.borrow();if let Some(v)=p.members.get(n){obj.members.insert(n.clone(),v.clone());}if let Some(v)=p.signatures.get(n){obj.signatures.insert(n.clone(),v.clone());}}
                    } Value::Object(Rc::new(RefCell::new(obj)))
                }
                "ProxyUtil.CreateProxy"=>{
                    if let (Some(Value::Object(private)),Some(proxy))=(args.first(),args.get(1)){private.borrow_mut().proxy=Some(proxy.clone());proxy.clone()}else{Value::Unknown}
                }
                "CreateFrame"|"CreateForbiddenFrame"=>Value::Widget("Frame".into()),
                "pairs"=>Value::Iterator(vec![String::new(),String::new()]),
                "ipairs"=>Value::Iterator(vec!["number".into(),String::new()]),
                _=>Value::Unknown,
            },_=>Value::Unknown,
        }
    }
    fn method(&mut self, receiver:Value,name:&str,args:Vec<Value>,depth:usize)->Value {
        if matches!(name,"CreateTexture"|"CreateFontString"|"CreateMaskTexture"|"CreateActor") {
            return Value::Widget(match name {"CreateTexture"=>"Texture","CreateFontString"=>"FontString","CreateMaskTexture"=>"MaskTexture",_=>"Actor"}.into());
        }
        if let Value::Object(o)=&receiver {
            if name=="ToProxy"{return o.borrow().proxy.clone().unwrap_or_default();}
            // Constructors store their allocator callback through Init. Preserve its
            // proven widget result on both the private object and exported proxy.
            if name=="Init" {
                for arg in &args {if matches!(arg,Value::Function(_)) && let Value::Widget(ty)=self.call(arg.clone(),vec![],depth+1) {
                    o.borrow_mut().element=Some(ty.clone());
                    if let Some(Value::Object(p))=&o.borrow().proxy{p.borrow_mut().element=Some(ty);}
                }}
            }
            let method=self.member(receiver.clone(),name);
            let mut params=args;
            if let Value::Function(id)=method {if self.functions[id].method{self.scopes.push(Scope{parent:Some(self.functions[id].scope),values:BTreeMap::from([("self".into(),receiver.clone())])});let mut f=self.functions[id].clone();f.scope=self.scopes.len()-1;let next=self.functions.len();self.functions.push(f);return self.call(Value::Function(next),params,depth+1);}}
            params.insert(0,receiver);return self.call(method,params,depth+1);
        } Value::Unknown
    }
    fn block(&mut self,b:&Block,scope:usize,depth:usize)->Option<Value>{
        if depth>24 || self.fuel==0{return None;}
        for stmt in b.stmts(){self.fuel=self.fuel.saturating_sub(1);if self.fuel==0{return None;}
            match stmt {
                Stmt::LocalAssignment(a)=>{let vs:Vec<_>=a.expressions().iter().map(|e|self.expr(e,scope,depth+1)).collect();for(i,n)in a.names().iter().enumerate(){self.scopes[scope].values.insert(n.to_string().trim().into(),vs.get(i).cloned().unwrap_or_default());}},
                Stmt::Assignment(a)=>{let vs:Vec<_>=a.expressions().iter().map(|e|self.expr(e,scope,depth+1)).collect();for(i,n)in a.variables().iter().enumerate(){let v=vs.get(i).cloned().unwrap_or_default();match n{
                    Var::Name(n)=>{let name=n.to_string();let name=name.trim();self.assign(scope,name,v.clone());if scope==1{self.exports.insert(name.into(),v);}},
                    Var::Expression(n)=>if let Some(Suffix::Index(Index::Dot{name,..}))=n.suffixes().last(){let receiver=match n.prefix(){Prefix::Name(t)=>self.get(scope,t.to_string().trim()),_=>Value::Unknown};if let Value::Object(o)=receiver{o.borrow_mut().members.insert(name.to_string().trim().into(),v);}},_=>{}
                }}},
                Stmt::LocalFunction(f)=>{let v=self.function(f.body(),scope,f.name().to_string().trim(),false);self.scopes[scope].values.insert(f.name().to_string().trim().into(),v);},
                Stmt::FunctionDeclaration(f)=>{
                    let names:Vec<_>=f.name().names().iter().map(|n|n.to_string().trim().to_owned()).collect();let method=f.name().method_name();let key=method.map(|m|m.to_string().trim().to_owned()).or_else(||names.get(1).cloned());
                    let v=self.function(f.body(),scope,key.as_deref().unwrap_or(&names[0]),method.is_some());
                    if let Some(key)=key{if let Value::Object(o)=self.get(scope,&names[0]){o.borrow_mut().members.insert(key,v);}}
                    else{self.assign(scope,&names[0],v.clone());self.exports.insert(names[0].clone(),v);}
                }
                Stmt::Do(d)=>{let child=self.scope(scope);if let Some(v)=self.block(d.block(),child,depth+1){return Some(v);}},
                Stmt::FunctionCall(c)=>{self.suffix(c.prefix(),c.suffixes(),scope,depth+1);},
                // Dynamic branches/loops are not executed or used as factory proof.
                _=>{}
            }
        }
        if let Some(LastStmt::Return(r))=b.last_stmt(){r.returns().iter().next().map(|e|self.expr(e,scope,depth+1))}else{None}
    }
}
fn declaration(name:&str,ty:&str)->Declaration{let mut d=super::xml::declaration(name,ty,"");d.attributes.clear();d}
fn return_type(v:Value)->Option<Declaration>{match v {
    Value::Widget(t)=>Some(declaration("result",&t)),Value::Number=>Some(declaration("result","number")),Value::Bool=>Some(declaration("result","bool")),
    Value::Iterator(types)=>{let mut d=declaration("result","Function");d.attributes.insert("SeluIterator".into(),serde_json::json!(types));Some(d)},_=>None
}}
/// Interpret only source files with constructors, with a fresh arena per file.
pub fn extend(m:&mut Manifest,files:&[(String,String)]) {
    for (file,text) in files {
        if !text.contains("CreateFromMixins") {continue;}
        let Ok(ast)=full_moon::parse(text.trim_start_matches('\u{feff}'))else{continue};
        let mut a=Analysis{scopes:vec![Scope{parent:None,values:BTreeMap::new()}],functions:vec![],exports:BTreeMap::new(),fuel:100_000,file:file.clone()};
        // Global mixins are already resolved from pinned source by the main scan.
        for d in &m.data_globals {
            let mut obj=Object::default();let mut pending=vec![d.name.clone()];let mut seen=std::collections::BTreeSet::new();
            while let Some(n)=pending.pop(){if !seen.insert(n.clone()){continue;}if let Some(d)=m.data_globals.iter().find(|d|d.name==n){pending.extend(d.inherits.iter().rev().cloned());for method in &d.methods{if let Some(sig)=m.function(&format!("{n}.{method}")).and_then(|f|f.signature.clone()){obj.signatures.entry(method.clone()).or_insert(sig);}}}}
            a.scopes[0].values.insert(d.name.clone(),Value::Object(Rc::new(RefCell::new(obj))));
        }
        let scope=a.scope(0);a.block(ast.nodes(),scope,0);
        let exports=a.exports.clone();
        for (name,f) in exports {
            if !name.starts_with("Create") || !matches!(f,Value::Function(_)){continue;}
            a.fuel=10_000;
            let result=a.call(f.clone(),vec![],0);
            let Value::Object(obj)=result else{continue;};let obj=obj.borrow().clone();
            if obj.members.is_empty() && obj.signatures.is_empty(){continue;}
            let type_name=format!("SourceObject_{name}");let mut widget=declaration(&type_name,"ScriptObject");
            widget.attributes.insert("file".into(),file.clone().into());
            let mut methods=obj.signatures.clone();
            for (n,v) in &obj.members {if let Value::Function(id)=v {
                let mut sig=a.functions[*id].signature.clone();sig.name=n.clone();a.fuel=2000;
                let value=a.method(Value::Object(Rc::new(RefCell::new(obj.clone()))),n,vec![],0);
                if let Some(ret)=return_type(value){sig.returns=vec![ret];}methods.insert(n.clone(),sig);
            }}
            // The allocator was proven by the factory closure, not by its name.
            if let Some(ty)=obj.element {
                if let Some(acquire)=methods.get_mut("Acquire"){let mut r=declaration("object",&ty);r.attributes.insert("Nilable".into(),true.into());acquire.returns=vec![r,declaration("new","bool")];}
                if let Some(release)=methods.get_mut("Release").and_then(|d|d.arguments.first_mut()){release.ty=Some(ty.clone());release.attributes.remove("Nilable");}
                if let Some(enumerate)=methods.get_mut("EnumerateActive"){let mut r=declaration("result","Function");r.attributes.insert("SeluIterator".into(),serde_json::json!([ty,"bool"]));enumerate.returns=vec![r];}
            }
            widget.methods=methods.into_values().collect();
            let Value::Function(id)=f else{unreachable!()};let mut signature=a.functions[id].signature.clone();signature.name=name.clone();signature.returns=vec![declaration("result",&type_name)];
            if let Some(entry)=m.functions.iter_mut().find(|f|f.name==name){if entry.signature.is_none(){entry.signature=Some(signature);m.widgets.push(widget);}}
        }
    }
}
