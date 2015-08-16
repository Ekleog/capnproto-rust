use schema_capnp::{brand, node, type_};
use codegen;
use codegen::{GeneratorContext};
use std::collections::hash_map::HashMap;

#[derive(Copy,Clone,PartialEq)]
pub enum Module {
    Reader(&'static str),
    Builder(&'static str),
    Owned,
    Client,
    Pipeline
}

impl ::std::fmt::Display for Module {
    fn fmt(&self, fmt:&mut ::std::fmt::Formatter) -> Result<(), ::std::fmt::Error> {
        let display_string = match self {
            &Module::Reader(lt) => format!("Reader<{}>", lt),
            &Module::Builder(lt) => format!("Builder<{}>", lt),
            &Module::Owned => "Owned".to_string(),
            &Module::Client => "Client".to_string(),
            &Module::Pipeline => "Pipeline".to_string(),
        };
        ::std::fmt::Display::fmt(&display_string, fmt)
    }
}

impl Module {
    fn bare_name(&self) -> &'static str {
        match self {
            &Module::Reader(_) => "Reader",
            &Module::Builder(_) => "Builder",
            &Module::Owned => "Owned",
            &Module::Client => "Client",
            &Module::Pipeline => "Pipeline",
        }
    }

    fn _have_lifetime(&self) -> bool {
        match self {
            &Module::Reader(_) | &Module::Builder(_) => true,
            &Module::Owned | &Module::Client | &Module::Pipeline => false,
        }
    }
}

pub struct TypeParameterTexts {
    pub expanded_list: Vec<String>,
    pub params: String,
    pub where_clause: String,
    pub where_clause_with_send: String,
    pub phantom_data: String
}

// this is a collection of helpers acting on a "Node" (most of them are Type definitions)
pub trait RustNodeInfo {
    fn type_string(&self, gen: &codegen::GeneratorContext, brand: &brand::Reader,
        scope:Option<&Vec<String>>, module:Module) -> String;

    // in rust, we have only nestable modules, and parameterizable types...
    // so a logically child struct will be a physical sibling to its logical parent. so it must
    // keep track of all parameterization on its own
    // this function recursively parse a node definition to find out the parameters used and inherited by scoping
    // and add them to the explicit parameters
    // the result is the actual parameters for the type to be generated
    fn expand_parameters(&self, gen: &::codegen::GeneratorContext) -> Vec<String>;

    fn parameters_texts(&self, gen: &::codegen::GeneratorContext) -> TypeParameterTexts;
}

// this is a collection of helpers acting on a "Type" (someplace where a Type is used, not defined)
pub trait RustTypeInfo {

    fn is_prim(&self) -> bool;
    fn is_parameterized(&self) -> bool;
    fn is_branded(&self) -> bool;
    fn type_string(&self, gen:&codegen::GeneratorContext, module:Module) -> String;
}

impl <'a> RustNodeInfo for node::Reader<'a> {

    fn expand_parameters(&self, gen:&::codegen::GeneratorContext) -> Vec<String> {
        let mut vec:Vec<String> = self.get_parameters().unwrap().iter().map(|p| p.get_name().unwrap().to_string()).collect();
        match self.which().unwrap() {
            ::schema_capnp::node::Struct(struct_reader) => {
                let fields = struct_reader.get_fields().unwrap();
                for field in fields.iter() {
                    match field.which().unwrap() {
                        ::schema_capnp::field::Slot(slot) => {
                            let typ = slot.get_type().unwrap().which().unwrap();
                            match typ {
                                ::schema_capnp::type_::Struct(st) => {
                                    let brand = st.get_brand().unwrap();
                                    let scopes = brand.get_scopes().unwrap();
                                    for scope in scopes.iter() {
                                        match scope.which().unwrap() {
                                            ::schema_capnp::brand::scope::Inherit(_) => {
                                                let parent_node = gen.node_map[&scope.get_scope_id()];
                                                for p in parent_node.get_parameters().unwrap().iter() {
                                                    let parameter_name = p.get_name().unwrap().to_string();
                                                    if !vec.contains(&parameter_name) {
                                                        vec.push(parameter_name);
                                                    }
                                                }
                                            },
                                            _ => {}
                                        }
                                    }
                                },
                                ::schema_capnp::type_::AnyPointer(any) => {
                                    match any.which().unwrap() {
                                        ::schema_capnp::type_::any_pointer::Parameter(def) => {
                                            let the_struct = &gen.node_map[&def.get_scope_id()];
                                            let parameters = the_struct.get_parameters().unwrap();
                                            let parameter = parameters.get(def.get_parameter_index() as u32);
                                            let parameter_name = parameter.get_name().unwrap().to_string();
                                            if !vec.contains(&parameter_name) {
                                                vec.push(parameter_name);
                                            }
                                        },
                                        _ => {}
                                    }
                                },
                                _ => {} // FIXME
                            }
                        },
                        _ => {} // FIXME
                    }
                }
            },
            ::schema_capnp::node::Interface(iface_reader) => {
                let methods = iface_reader.get_methods().unwrap();
                for method in methods.iter() {
                    for brand in vec!(method.get_param_brand().unwrap(), method.get_result_brand().unwrap()) {
                        for scope in brand.get_scopes().unwrap().iter() {
                            match scope.which().unwrap() {
                                ::schema_capnp::brand::scope::Inherit(_) => {
                                    let parent_node = gen.node_map[&scope.get_scope_id()];
                                    for p in parent_node.get_parameters().unwrap().iter() {
                                        let parameter_name = p.get_name().unwrap().to_string();
                                        if !vec.contains(&parameter_name) {
                                            vec.push(parameter_name);
                                        }
                                    }
                                },
                                _ => {}
                            }
                        }
                    }
                }
            }
            _ => {} // FIXME
        }
        vec
    }

    fn parameters_texts(&self, gen:&::codegen::GeneratorContext) -> TypeParameterTexts {
        if self.get_is_generic() {
            let params = self.expand_parameters(&gen);
            let type_parameters = params.iter().map(|param| {
                format!("{}",param)
            }).collect::<Vec<String>>().connect(",");
            let where_clause = "where ".to_string() + &*(params.iter().map(|param| {
                format!("{}: for<'c> ::capnp::traits::Owned<'c>", param)
            }).collect::<Vec<String>>().connect(", ") + " ");
            let where_clause_with_send = "where ".to_string() + &*(params.iter().map(|param| {
                //format!("{}Reader:Send+FromPointerReader<'a>", param)
                format!("{}Reader:Send", param)
            }).collect::<Vec<String>>().connect(", ") + " ") + ", "
                + &*(params.iter().map(|param| {
                //format!("{}Builder:Send+FromPointerBuilder<'a>", param)
                format!("{}Builder:Send", param)
            }).collect::<Vec<String>>().connect(", ") + " ");
            let phantom_data = "_phantom: PhantomData,".to_string();

            TypeParameterTexts {
                expanded_list: params,
                params: type_parameters,
                where_clause: where_clause,
                where_clause_with_send: where_clause_with_send,
                phantom_data: phantom_data
            }
        } else {
            TypeParameterTexts {
                expanded_list: vec!(),
                params: "".to_string(),
                where_clause: "".to_string(),
                where_clause_with_send: "".to_string(),
                phantom_data: "".to_string(),
            }
        }
    }

    fn type_string(&self, gen:&codegen::GeneratorContext, brand:&::schema_capnp::brand::Reader,
            scope:Option<&Vec<String>>, module:Module) -> String {
//        unimplemented!();
        let the_mod = scope.unwrap_or_else( || &gen.scope_map[&self.get_id()]).connect("::");
        let mut reader_bindings:Vec<String> = vec!();
        let mut builder_bindings:Vec<String> = vec!();
        let parameters = self.expand_parameters(gen);
        for s in brand.get_scopes().unwrap().iter() {
            match s.which().unwrap() {
                brand::scope::Inherit(_) => {
                    let parent_node = gen.node_map[&s.get_scope_id()];
                    for p in parent_node.get_parameters().unwrap().iter() {
                        if parameters.contains(&p.get_name().unwrap().to_string()) {
                            reader_bindings.push(p.get_name().unwrap().to_string());
                            builder_bindings.push(p.get_name().unwrap().to_string());
                        }
                    }
                },
                brand::scope::Bind(b) => {
                    b.unwrap().iter().map(|binding|
                        match binding.which().unwrap() {
                            brand::binding::Type(Ok(t)) => {
                                reader_bindings.push(t.type_string(gen, Module::Owned));
                                builder_bindings.push(t.type_string(gen, Module::Owned));
                            }
                            _ => {}
                        }
                    ).count();
                },
            }
        };
        let local_lifetime = match module {
            Module::Reader(lt) => lt,
            Module::Builder(lt) => lt,
            _ => "",
        };

        let lifetime_coma = if local_lifetime == "" { "".to_string() } else {
            format!("{},", local_lifetime)
        };
        if parameters.len() == 0 {
            format!("{}::{}", the_mod, module)
        } else {
            if parameters.len() != reader_bindings.len() {
                reader_bindings.clear();
                builder_bindings.clear();
                for _ in 0 .. parameters.len() {
                    reader_bindings.push(format!("::capnp::any_pointer::Reader<{}>", local_lifetime));
                    builder_bindings.push(format!("::capnp::any_pointer::Builder<{}>", local_lifetime));
                }
            }
            format!("{}::{}<{}{}>", the_mod, module.bare_name(), lifetime_coma, reader_bindings.connect(","))
        }
    }
}

impl <'a> RustTypeInfo for type_::Reader<'a> {

    fn type_string(&self, gen:&codegen::GeneratorContext, module:Module) -> String {
        use codegen_types::RustTypeInfo;

        let local_lifetime = match module {
            Module::Reader(lt) => lt,
            Module::Builder(lt) => lt,
            _ => "",
        };

        let lifetime_coma = if local_lifetime == "" { "".to_string() } else {
            format!("{},", local_lifetime)
        };

        match self.which().unwrap() {
            type_::Void(()) => "()".to_string(),
            type_::Bool(()) => "bool".to_string(),
            type_::Int8(()) => "i8".to_string(),
            type_::Int16(()) => "i16".to_string(),
            type_::Int32(()) => "i32".to_string(),
            type_::Int64(()) => "i64".to_string(),
            type_::Uint8(()) => "u8".to_string(),
            type_::Uint16(()) => "u16".to_string(),
            type_::Uint32(()) => "u32".to_string(),
            type_::Uint64(()) => "u64".to_string(),
            type_::Float32(()) => "f32".to_string(),
            type_::Float64(()) => "f64".to_string(),
            type_::Text(()) => format!("text::{}", module),
            type_::Data(()) => format!("data::{}", module),
            type_::Struct(st) => do_branding(gen, st.get_type_id(), st.get_brand().unwrap(), module),
            type_::Interface(interface) => {
                do_branding(gen, interface.get_type_id(), interface.get_brand().unwrap(), module)
            },
            type_::List(ot1) => {
                match ot1.get_element_type().unwrap().which() {
                    Err(_) => { panic!("unsupported type") },
                    Ok(type_::Struct(_)) => {
                        let inner = ot1.get_element_type().unwrap().type_string(gen, Module::Owned);
                        format!("struct_list::{}<{}{}>", module.bare_name(), lifetime_coma, inner)
                    },
                    Ok(type_::Enum(_)) => {
                        let inner = ot1.get_element_type().unwrap().type_string(gen, Module::Owned);
                        format!("enum_list::{}<{}{}>", module.bare_name(), lifetime_coma, inner)
                    },
                    Ok(type_::List(_)) => {
                        let inner = ot1.get_element_type().unwrap().type_string(gen, Module::Owned);
                        format!("list_list::{}<{}{}>", module.bare_name(), lifetime_coma, inner)
                    },
                    Ok(type_::Text(())) => {
                        format!("text_list::{}", module)
                    },
                    Ok(type_::Data(())) => {
                        format!("data_list::{}", module)
                    },
                    Ok(type_::Interface(_)) => {panic!("unimplemented") },
                    Ok(type_::AnyPointer(_)) => {panic!("List(AnyPointer) is unsupported")},
                    Ok(_) => {
                        let inner = ot1.get_element_type().unwrap().type_string(gen, Module::Owned);
                        format!("primitive_list::{}<{}{}>", module.bare_name(), lifetime_coma, inner)
                    },
                }
            },
            type_::Enum(en) => {
                let scope = &gen.scope_map[&en.get_type_id()];
                scope.connect("::").to_string()
            },
            type_::AnyPointer(pointer) => {
                match pointer.which().unwrap() {
                    type_::any_pointer::Parameter(def) => {
                        let the_struct = &gen.node_map[&def.get_scope_id()];
                        let parameters = the_struct.get_parameters().unwrap();
                        let parameter = parameters.get(def.get_parameter_index() as u32);
                        let parameter_name = parameter.get_name().unwrap();
                        match module {
                            Module::Owned => parameter_name.to_string(),
                            Module::Reader(lifetime) => {
                                format!("<{} as ::capnp::traits::Owned<{}>>::Reader",
                                        parameter_name, lifetime)
                            }
                            Module::Builder(lifetime) => {
                                format!("<{} as ::capnp::traits::Owned<{}>>::Builder",
                                        parameter_name, lifetime)
                            }
                            Module::Pipeline => {
                                format!("<{} as ::capnp::traits::Owned<'static>>::Pipeline", parameter_name)
                            }
                            _ => { unimplemented!() }
                        }
                    },
                    _ => {
                        match module {
                            Module::Reader(lifetime) => {
                                format!("::capnp::any_pointer::Reader<{}>", lifetime)
                            }
                            Module::Builder(lifetime) => {
                                format!("::capnp::any_pointer::Builder<{}>", lifetime)
                            }
                            _ => {
                                format!("::capnp::any_pointer::{}", module)
                            }
                        }
                    }
                }
            }
        }
    }

    fn is_parameterized(&self) -> bool {
        match self.which().unwrap() {
            type_::AnyPointer(pointer) => {
                match pointer.which().unwrap() {
                    type_::any_pointer::Parameter(_) => true,
                    _ => false
                }
            }
            _ => false
        }
    }

    fn is_branded(&self) -> bool {
        match self.which().unwrap() {
            type_::Struct(st) => {
                let brand = st.get_brand().unwrap();
                let scopes = brand.get_scopes().unwrap();
                scopes.len() > 0
            }
            _ => false
        }
    }

    #[inline(always)]
    fn is_prim(&self) -> bool {
        match self.which().unwrap() {
            type_::Int8(()) | type_::Int16(()) | type_::Int32(()) | type_::Int64(()) |
            type_::Uint8(()) | type_::Uint16(()) | type_::Uint32(()) | type_::Uint64(()) |
            type_::Float32(()) | type_::Float64(()) | type_::Void(()) | type_::Bool(()) => true,
            _ => false
        }
    }
}

///
///
fn do_branding(gen: &GeneratorContext, node_id: u64, brand: brand::Reader, leaf: Module) -> String {
    let scopes = brand.get_scopes().unwrap();
    let mut brand_scopes = HashMap::new();
    for scope in scopes.iter() {
        brand_scopes.insert(scope.get_scope_id(), scope);
    }
    let brand_scopes = brand_scopes; // freeze
    let mut current_node_id = node_id;
    let mut accumulator: Vec<Vec<String>> = Vec::new();
    loop {
        let current_node = match gen.node_map.get(&current_node_id) {
            None => break,
            Some(node) => node,
        };
        let params = current_node.get_parameters().unwrap();
        let mut arguments: Vec<String> = Vec::new();
        match brand_scopes.get(&current_node_id) {
            None => {
                for _ in params.iter() {
                    arguments.push("::capnp::any_pointer::Owned".to_string());
                }
            },
            Some(scope) => {
                match scope.which().unwrap() {
                    brand::scope::Inherit(()) => {
                        for param in params.iter() {
                            arguments.push(param.get_name().unwrap().to_string());
                        }
                    }
                    brand::scope::Bind(bindings_list_opt) => {
                        let bindings_list = bindings_list_opt.unwrap();
                        assert_eq!(bindings_list.len(), params.len());
                        for binding in bindings_list.iter() {
                            match binding.which().unwrap() {
                                brand::binding::Unbound(()) => {
                                    arguments.push("::capnp::any_pointer::Owned".to_string());
                                }
                                brand::binding::Type(t) => {
                                    arguments.push(t.unwrap().type_string(gen, Module::Owned));
                                }
                            }
                        }
                    }
                }
            }
        }
        accumulator.push(arguments);
        current_node_id = current_node.get_scope_id();
    }

    // Now add a lifetime paramter if the leaf has one.
    match leaf {
        Module::Reader(lt) => accumulator.push(vec!(lt.to_string())),
        Module::Builder(lt) => accumulator.push(vec!(lt.to_string())),
        _ => (),
    }

    accumulator.reverse();

    let arguments = if accumulator.len() > 0 {
        format!("<{}>", accumulator.concat().connect(","))
    } else {
        "".to_string()
    };

    return format!("{}::{}{}", gen.scope_map[&node_id].connect("::"),
                   leaf.bare_name().to_string(), arguments);
}

