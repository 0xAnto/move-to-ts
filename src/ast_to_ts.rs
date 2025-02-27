use crate::ast_exp::*;
use crate::ast_tests::check_test;
use crate::shared::*;
use crate::tsgen_writer::TsgenWriter;
use crate::utils::{capitalize, get_iterable_table_helper_decl, get_table_helper_decl, rename};
use itertools::Itertools;
use move_compiler::shared::Name;
use move_compiler::{
    diagnostics::{Diagnostic, Diagnostics},
    expansion::ast::{Attribute_, Attributes, ModuleIdent},
    hlir::ast::*,
    naming::ast::{BuiltinTypeName_, StructTypeParameter},
    parser::ast::{Ability_, ConstantName, FunctionName, StructName, Var},
};
use move_ir_types::location::Loc;
use std::collections::BTreeSet;

pub fn translate_module(
    mident: ModuleIdent,
    mdef: &ModuleDefinition,
    c: &mut Context,
) -> Result<(String, String), Diagnostics> {
    let filename = format!(
        "{}/{}.ts",
        format_address(mident.value.address),
        mident.value.module
    );
    c.reset_for_module(mident);
    let content = to_ts_string(&(mident, mdef), c);
    match content {
        Err(diag) => {
            let mut diags = Diagnostics::new();
            diags.add(diag);
            Err(diags)
        }
        Ok(res) => Ok((filename, res)),
    }
}

pub fn to_ts_string(v: &impl AstTsPrinter, c: &mut Context) -> Result<String, Diagnostic> {
    let mut writer = TsgenWriter::new();
    v.write_ts(&mut writer, c)?;
    let mut lines = vec![
        "import * as $ from \"@manahippo/move-to-ts\";".to_string(),
        "import {AptosDataCache, AptosParserRepo, DummyCache} from \"@manahippo/move-to-ts\";"
            .to_string(),
        "import {U8, U64, U128} from \"@manahippo/move-to-ts\";".to_string(),
        "import {u8, u64, u128} from \"@manahippo/move-to-ts\";".to_string(),
        "import {TypeParamDeclType, FieldDeclType} from \"@manahippo/move-to-ts\";".to_string(),
        "import {AtomicTypeTag, StructTag, TypeTag, VectorTag} from \"@manahippo/move-to-ts\";"
            .to_string(),
        "import {HexString, AptosClient, AptosAccount} from \"aptos\";".to_string(),
    ];
    for package_name in c.package_imports.iter() {
        lines.push(format!(
            "import * as {} from \"../{}\";",
            capitalize(package_name),
            package_name
        ));
    }
    for module_name in c.same_package_imports.iter() {
        lines.push(format!(
            "import * as {} from \"./{}\";",
            capitalize(module_name),
            module_name
        ));
    }
    lines.push(format!("{}", writer));
    Ok(lines.join("\n"))
}

pub fn handle_special_module(
    mi: &ModuleIdent,
    _module: &ModuleDefinition,
    w: &mut TsgenWriter,
    _c: &mut Context,
) -> WriteResult {
    if format_address_hex(mi.value.address) == "0x1" {
        if mi.value.module.to_string() == "table" {
            w.writeln(get_table_helper_decl());
        } else if mi.value.module.to_string() == "iterable_table" {
            w.writeln(get_iterable_table_helper_decl());
        }
    }
    Ok(())
}

impl AstTsPrinter for (ModuleIdent, &ModuleDefinition) {
    const CTOR_NAME: &'static str = "ModuleDefinition";
    fn write_ts(&self, w: &mut TsgenWriter, c: &mut Context) -> WriteResult {
        /*
        - imports (handled by TsgenWriter)
        - constants
        - structs
        - functions
         */
        let (name, module) = self;
        let ModuleDefinition {
            package_name,
            attributes: _,
            is_source_module: _,
            dependency_order: _,
            friends: _,
            structs,
            constants,
            functions,
        } = module;

        let package_name = package_name.map_or("".to_string(), |symbol| symbol.to_string());

        // module meta
        w.export_const("packageName", quote(&package_name));
        w.export_const(
            "moduleAddress",
            format!(
                "new HexString({})",
                quote(&format_address_hex(name.value.address))
            ),
        );
        w.export_const("moduleName", quote(&name.value.module.0));
        w.new_line();

        // constants
        for (cname, cdef) in constants.key_cloned_iter() {
            (cname, cdef).write_ts(w, c)?;
        }
        w.new_line();

        // structs
        for (sname, sdef) in structs.key_cloned_iter() {
            (sname, sdef).write_ts(w, c)?;
        }

        // functions
        for (fname, fdef) in functions.key_cloned_iter() {
            (fname, fdef).write_ts(w, c)?;
        }

        // loadParsers
        write_load_parsers(name, module, w, c)?;

        if c.config.ui {}

        // for things like Table, IterableTable
        handle_special_module(name, module, w, c)?;

        Ok(())
    }
}

pub fn write_load_parsers(
    mident: &ModuleIdent,
    module: &ModuleDefinition,
    w: &mut TsgenWriter,
    _c: &mut Context,
) -> WriteResult {
    w.writeln("export function loadParsers(repo: AptosParserRepo) {");

    for (sname, _) in module.structs.key_cloned_iter() {
        let paramless_name = format!(
            "{}::{}::{}",
            format_address_hex(mident.value.address),
            mident.value.module,
            sname
        );
        w.writeln(format!(
            "  repo.addParser({}, {}.{}Parser);",
            quote(&paramless_name),
            sname,
            sname
        ));
    }

    w.writeln("}");

    Ok(())
}

impl AstTsPrinter for ConstantName {
    const CTOR_NAME: &'static str = "_ConstantName";
    fn term(&self, _c: &mut Context) -> TermResult {
        Ok(rename(self))
    }
}

impl AstTsPrinter for FunctionName {
    const CTOR_NAME: &'static str = "_FunctionName";
    fn term(&self, _c: &mut Context) -> TermResult {
        Ok(rename(self))
    }
}

impl AstTsPrinter for StructName {
    const CTOR_NAME: &'static str = "_StructName";
    fn term(&self, _c: &mut Context) -> TermResult {
        Ok(rename(self))
    }
}

pub fn write_simplify_constant_block(
    block: &Block,
    w: &mut TsgenWriter,
    c: &mut Context,
) -> WriteResult {
    if block.len() == 1 {
        match &block[0].value {
            Statement_::Command(cmd) => match &cmd.value {
                Command_::Return { from_user: _, exp } => {
                    w.write(exp.term(c)?);
                    return Ok(());
                }
                _ => (),
            },
            _ => (),
        }
    }
    // write block as lambda
    w.write("( () => ");
    block.write_ts(w, c)?;
    w.write(")()");
    Ok(())
}

impl AstTsPrinter for (ConstantName, &Constant) {
    const CTOR_NAME: &'static str = "ConstantDef";
    fn write_ts(&self, w: &mut TsgenWriter, c: &mut Context) -> WriteResult {
        let (
            name,
            Constant {
                attributes: _,
                loc: _loc,
                signature,
                value,
            },
        ) = self;
        let (_, value_block) = value;
        let typename = ts_constant_type(signature, c)?;
        w.write(format!("export const {} : {} = ", name.term(c)?, typename));
        // FIXME this is a block
        write_simplify_constant_block(value_block, w, c)?;
        w.writeln(";");
        Ok(())
    }
}

impl AstTsPrinter for StructTypeParameter {
    // only used by (StructName, &StructDefinition)
    const CTOR_NAME: &'static str = "StructTypeParameter";
    fn term(&self, _c: &mut Context) -> TermResult {
        let Self { is_phantom, param } = self;
        let name = rename(&quote(&param.user_specified_name));
        Ok(format!("{{ name: {}, isPhantom: {} }}", name, is_phantom))
    }
}

pub fn handle_special_structs(
    name: &StructName,
    w: &mut TsgenWriter,
    c: &mut Context,
) -> WriteResult {
    if c.current_module.is_none() {
        return Ok(());
    }
    let mident = c.current_module.unwrap();
    let package_name = format_address(mident.value.address);
    if package_name == "std" {
        if mident.value.module.to_string() == "string" && name.to_string() == "String" {
            w.writeln("str(): string { return $.u8str(this.bytes); }");
        }
    } else if package_name == "aptos_std" {
        if mident.value.module.to_string() == "iterable_table"
            && name.to_string() == "IterableTable"
        {
            w.writeln("toTypedIterTable<K, V>(field: $.FieldDeclType) { return (TypedIterableTable<K, V>).buildFromField(this, field); }");
        }
        if mident.value.module.to_string() == "type_info" && name.to_string() == "TypeInfo" {
            w.writeln("typeFullname(): string {");
            w.writeln("  return `${this.account_address.toShortString()}::${$.u8str(this.module_name)}::${$.u8str(this.struct_name)}`;");
            w.writeln("}");
            w.writeln("toTypeTag() { return $.parseTypeTagOrThrow(this.typeFullname()); }");
            w.writeln("moduleName() { return (this.toTypeTag() as $.StructTag).module; }");
            w.writeln("structName() { return (this.toTypeTag() as $.StructTag).name; }");
        }
    }
    Ok(())
}

pub fn handle_struct_show_iter_table_directive(
    sname: &StructName,
    sdef: &StructDefinition,
    inner_attrs: &Attributes,
    w: &mut TsgenWriter,
    c: &mut Context,
) -> WriteResult {
    for (_, pattr) in inner_attrs.key_cloned_iter() {
        match &pattr.value {
            Attribute_::Name(field_name) => {
                // validate this at end-of-module generation
                c.add_show_iter_table(&c.current_module.unwrap(), sname, sdef, field_name);

                // generate show method
                w.new_line();

                let fields = match &sdef.fields {
                    StructFields::Defined(fields) => fields,
                    StructFields::Native(_) => {
                        return derr!((
                            field_name.loc,
                            "cannot show iterable tables from native struct"
                        ));
                    }
                };

                let field_opt = fields
                    .into_iter()
                    .find(|(f_name, _)| f_name.to_string() == field_name.to_string());

                if field_opt.is_none() {
                    return derr!((
                        field_name.loc,
                        format!("Field {} does not exist", field_name)
                    ));
                }
                let (field_decl_name, table_base) = field_opt.unwrap();

                let table_targs_opt = match &table_base.value {
                    BaseType_::Apply(_, typename, targs) => match &typename.value {
                        TypeName_::ModuleType(table_mi, table_sname) => {
                            if format_address_hex(table_mi.value.address) != "0x1"
                                || table_mi.value.module.to_string() != "iterable_table"
                                || table_sname.to_string() != "IterableTable"
                            {
                                None
                            } else {
                                Some(targs)
                            }
                        }
                        _ => None,
                    },
                    _ => None,
                };

                if table_targs_opt.is_none() {
                    return derr!((
                        field_name.loc,
                        format!("Field {} is not an IterableTable", field_name)
                    ));
                }

                let table_targs = table_targs_opt.unwrap();
                if table_targs.len() != 2 {
                    return derr!((
                        field_decl_name.0.loc,
                        "IterableTable should have 2 type arguments "
                    ));
                }
                let key_ts_type = base_type_to_tstype(&table_targs[0], c)?;
                let value_ts_type = base_type_to_tstype(&table_targs[1], c)?;

                w.writeln(format!(
                    "async getIterTableEntries_{}(client: AptosClient, repo: AptosParserRepo) {{",
                    field_name
                ));
                w.writeln(format!("  const cache = new DummyCache();"));
                w.writeln(format!(
                    "  const tags = (this.typeTag as StructTag).typeParams;"
                ));
                w.writeln(format!(
                    "  const iterTableField = {}.fields.filter(f=>f.name === '{}')[0]",
                    sname, field_name
                ));
                w.writeln(format!(
                    "  const typedIterTable = this.{}.toTypedIterTable<{},{}>(iterTableField);",
                    field_name, key_ts_type, value_ts_type,
                ));
                w.writeln(format!(
                    "  return await typedIterTable.fetchAll(client, repo);"
                ));
                w.writeln("}");
            }
            _ => {
                return derr!((
                    pattr.loc,
                    "show_iter_table directive expects only a field name as argument"
                ));
            }
        }
    }

    Ok(())
}

pub fn validate_method(
    sname: &StructName,
    sdef: &StructDefinition,
    name: &Name,
    f: &Function,
    c: &mut Context,
) -> WriteResult {
    // expect the fname to be a valid function, whose signature is:
    // fname<sdef.type_params>(obj: &sdef)
    let err = derr!((
        name.loc,
        format!(
            "This function should have &{} as its first parameter",
            sname
        )
    ));
    let sig = &f.signature;
    // check it has the same type parameters as sdef
    if sig.type_parameters.len() != sdef.type_parameters.len() {
        return derr!((
            name.loc,
            format!(
                "This function should have the same type parameters as {}",
                sname
            )
        ));
    }
    for (idx, tparam) in sig.type_parameters.iter().enumerate() {
        if sdef.type_parameters[idx].param.user_specified_name != tparam.user_specified_name {
            return derr!((tparam.user_specified_name.loc, "Mismatched type parameters"));
        }
    }
    // check it has at least one parameter of sdef's type
    if sig.parameters.len() < 1 {
        return err;
    }
    let base = match &sig.parameters[0].1.value {
        SingleType_::Base(b) => b,
        SingleType_::Ref(_, b) => b,
    };
    if let BaseType_::Apply(_, typename, targs) = &base.value {
        match &typename.value {
            TypeName_::ModuleType(mi, sname2) => {
                if is_same_module(&c.current_module.unwrap(), mi) && *sname == *sname2 {
                    // check type params are in the right order
                    for (idx, tparam) in targs.iter().enumerate() {
                        match &tparam.value {
                            BaseType_::Param(tp) => {
                                if sdef.type_parameters[idx].param.user_specified_name
                                    != tp.user_specified_name
                                {
                                    return derr!((tparam.loc, "Mismatched type parameters"));
                                }
                            }
                            _ => {
                                return derr!((tparam.loc, "Mismatched type parameters"));
                            }
                        }
                    }
                    // good
                } else {
                    return err;
                }
            }
            _ => return err,
        }
    } else {
        return err;
    }

    Ok(())
}

pub fn handle_struct_method_directive(
    sname: &StructName,
    sdef: &StructDefinition,
    inner_attrs: &Attributes,
    w: &mut TsgenWriter,
    c: &mut Context,
) -> WriteResult {
    for (_, pattr) in inner_attrs.key_cloned_iter() {
        match &pattr.value {
            Attribute_::Name(fname) => {
                // validate it now
                let program = c.program.clone();
                let mdef = program.modules.get(&c.current_module.unwrap()).unwrap();
                let func_opt = mdef.functions.get(&FunctionName(*fname));

                if func_opt.is_none() {
                    return derr!((fname.loc, "This function does not exist in current module"));
                }
                let func = func_opt.unwrap();
                validate_method(sname, sdef, fname, func, c)?;

                // generate method
                w.new_line();

                let async_modifier = if c.is_async() { "async " } else { "" };
                w.writeln(format!("{}{}(", async_modifier, fname));
                write_parameters(&func.signature, w, c, false, true)?;
                w.writeln(") {");
                w.writeln(format!("  const cache = new DummyCache();"));
                w.writeln(format!(
                    "  const tags = (this.typeTag as StructTag).typeParams;"
                ));
                let args_str = func.signature.parameters[1..]
                    .iter()
                    .map(|(v, _)| v.to_string())
                    .join(", ");
                w.writeln(format!(
                    "  return {}(this, {}{}cache{});",
                    format_function_name(fname, c.is_async()),
                    args_str,
                    if args_str.is_empty() { "" } else { ", " },
                    if func.signature.type_parameters.is_empty() {""} else {", tags"},
                ));
                w.writeln("}");

                // generate printer for cli
                if func.signature.return_type.value != Type_::Unit {
                    c.add_printer_method(
                        &c.current_module.unwrap(),
                        sname,
                        sdef,
                        fname,
                        &func.signature,
                    );
                }
            }
            _ => {
                return derr!((
                    pattr.loc,
                    "show directive expects only a list of function names as argument"
                ));
            }
        }
    }

    Ok(())
}

pub fn handle_struct_directives(
    sname: &StructName,
    sdef: &StructDefinition,
    w: &mut TsgenWriter,
    c: &mut Context,
) -> WriteResult {
    let attrs = &sdef.attributes;
    for (name, attr) in attrs.key_cloned_iter() {
        match name.to_string().as_str() {
            "cmd" => return derr!((attr.loc, "the 'cmd' attribute cannot be used on structs")),
            "method" => match &attr.value {
                Attribute_::Parameterized(_, inner_attrs) => {
                    w.new_line();
                    handle_struct_method_directive(sname, sdef, inner_attrs, w, c)?;
                }
                _ => {
                    return derr!((attr.loc, "the 'method' attribute requires a list of function names as argument (e.g. $[method(show_x_as_y)]"))
                }
            }
            "show_iter_table" => match &attr.value {
                Attribute_::Parameterized(_, inner_attrs) => {
                    w.new_line();
                    handle_struct_show_iter_table_directive(sname, sdef, inner_attrs, w, c)?;
                }
                _ => {
                    return derr!((attr.loc, "the 'show' requires a list of function names as argument (e.g. $[show(show_x_as_y)]"))
                }
            }
            _ => (),
        }
    }
    Ok(())
}

impl AstTsPrinter for (StructName, &StructDefinition) {
    const CTOR_NAME: &'static str = "StructDef";
    fn write_ts(&self, w: &mut TsgenWriter, c: &mut Context) -> WriteResult {
        let (name, sdef) = self;

        w.new_line();
        w.writeln(format!("export class {} ", name.term(c)?));
        w.short_block(|w| {
            w.writeln("static moduleAddress = moduleAddress;");
            w.writeln("static moduleName = moduleName;");
            w.writeln(format!("static structName: string = {};", quote(&name.term(c)?)));

            // 0. type parameters
            // 1. static field decl
            // 2. actual field decl
            // 3. ctor
            // 4. static parser
            // 5. resource loader
            // 6. additional util funcs
            // 7. attribute-directives

            // 0: type parameters
            w.write("static typeParameters: TypeParamDeclType[] = [");
            w.indent(2, |w| {
                w.list(&sdef.type_parameters, ",", |w, struct_tparam| {
                    w.write(struct_tparam.term(c)?);
                    Ok(true)
                })?;
                Ok(())
            })?;
            w.writeln("];");
            match &sdef.fields {
                StructFields::Native(_) => (),
                StructFields::Defined(fields) => {

                    // 1: static field decls
                    w.writeln("static fields: FieldDeclType[] = [");
                    w.list(fields, ",", |w, (name, ty)| {
                        w.write(format!(
                            "{{ name: {}, typeTag: {} }}",
                            quote(&rename(&name)),
                            base_type_to_typetag_builder(ty, &sdef.type_parameters, c)?
                        ));
                        Ok(true)
                    })?;
                    w.writeln("];");
                    w.new_line();

                    // 2. actual class fields
                    if !fields.is_empty() {
                        w.list(fields, "", |w, (name, ty)| {
                            w.write(format!("{}: {};", rename(&name), base_type_to_tstype(ty, c)?));
                            Ok(true)
                        })?;
                        w.new_line();
                        w.new_line();
                    }

                    // 3. ctor
                    w.write("constructor(proto: any, public typeTag: TypeTag) {");
                    w.indent(2, |w| {
                        // one line for each field
                        w.list(fields, "", |w, (name, ty)| {
                            let name = rename(&name);
                            let tstype = base_type_to_tstype(ty, c)?;
                            w.write(
                                format!("this.{} = proto['{}'] as {};", name, name, tstype));
                            Ok(true)
                        })?;
                        Ok(())
                    })?;
                    w.writeln("}");

                    // 4. static Parser
                    w.new_line();
                    w.writeln(format!("static {}Parser(data:any, typeTag: TypeTag, repo: AptosParserRepo) : {} {{", name, name));
                    w.writeln(format!("  const proto = $.parseStructProto(data, typeTag, repo, {});", name));
                    w.writeln(format!("  return new {}(proto, typeTag);", name));
                    w.writeln("}");

                    // 5. resource loader
                    if sdef.abilities.has_ability_(Ability_::Key) {
                        w.new_line();
                        w.writeln("static async load(repo: AptosParserRepo, client: AptosClient, address: HexString, typeParams: TypeTag[]) {");
                        w.writeln(format!("  const result = await repo.loadResource(client, address, {}, typeParams);", name));
                        w.writeln(format!("  return result as unknown as {};", name));
                        w.write("}");
                    }

                    // 6. additional util funcs
                    handle_special_structs(&name, w, c)?;

                    // 7. attribute directives
                    handle_struct_directives(&name, sdef, w, c)?;
                }
            };
            Ok(())
        })?;
        w.new_line();

        Ok(())
    }
}

pub fn write_parameters(
    sig: &FunctionSignature,
    w: &mut TsgenWriter,
    c: &mut Context,
    skip_signer: bool,
    skip_first: bool,
) -> WriteResult {
    w.increase_indent();
    for (idx, (name, ty)) in sig.parameters.iter().enumerate() {
        if skip_signer && is_type_signer(ty) {
            continue;
        }
        if skip_first && idx == 0 {
            continue;
        }
        w.writeln(format!(
            "{}: {},",
            rename(name),
            single_type_to_tstype(ty, c)?
        ));
    }
    w.decrease_indent();

    Ok(())
}

pub fn handle_function_cmd_directive(
    fname: &FunctionName,
    f: &Function,
    inner_attrs: Option<&Attributes>,
    _w: &mut TsgenWriter,
    c: &mut Context,
) -> WriteResult {
    if f.entry.is_none() {
        return derr!((
            fname.0.loc,
            "the cmd attribute only works on public entry functions"
        ));
    }
    let mut desc = None;
    if let Some(params) = inner_attrs {
        for (pname, pattr) in params.key_cloned_iter() {
            match pname.to_string().as_str() {
                "desc" => {
                    if let Some(str_desc) = extract_attribute_value_string(pattr) {
                        desc = Some(str_desc);
                    } else {
                        return derr!((
                            pattr.loc,
                            "desc needs to be assigned a byte string value (e.g. b\"description\")"
                        ));
                    }
                }
                _ => {
                    return derr!((pname.loc, "Unrecognized parameter to cmd directive"));
                }
            }
        }
    }
    c.add_cmd(&c.current_module.unwrap(), fname, f, desc);

    Ok(())
}

pub fn write_query_function(
    fname: &FunctionName,
    f: &Function,
    return_type: &BaseType,
    w: &mut TsgenWriter,
    c: &mut Context,
) -> WriteResult {
    let query_fname = format!("query_{}", fname);
    w.writeln(format!("export async function {}(", query_fname));
    w.increase_indent();

    // params
    w.writeln("client: AptosClient,");
    w.writeln("account: AptosAccount,");
    w.writeln("repo: AptosParserRepo,");
    write_parameters(&f.signature, w, c, true, false)?;
    w.writeln("$p: TypeTag[],");

    w.decrease_indent();
    w.writeln(") {");

    let mut param_list = f
        .signature
        .parameters
        .iter()
        .filter(|(_, t)| !is_type_signer(t))
        .map(|(v, _)| v.to_string())
        .collect::<Vec<_>>();

    if !f.signature.type_parameters.is_empty() {
        param_list.push("$p".to_string());
    }

    let move_to_err = derr!((return_type.loc, "Expect move_to to contain a struct type"));
    let output_struct_name = match &return_type.value {
        BaseType_::Apply(_, tn, _) => match &tn.value {
            TypeName_::ModuleType(_, name) => name.to_string(),
            _ => {
                return move_to_err;
            },
        }
        _ => {
            return move_to_err;
        }
    };

    w.increase_indent();

    // body
    w.writeln(format!(
        "const payload = buildPayload_{}({});",
        fname, param_list.join(", ")
    ));
    let output_tag = base_type_to_typetag(return_type, c)?;
    w.writeln(format!("const outputTypeTag = {};", output_tag));
    w.writeln("const output = await $.simulatePayloadTx(client, account, payload);");
    w.writeln(format!("return $.takeSimulationValue<{}>(output, outputTypeTag, repo)", output_struct_name));

    w.decrease_indent();
    w.writeln("}");

    Ok(())
}

pub fn handle_function_query_directive(
    fname: &FunctionName,
    f: &Function,
    w: &mut TsgenWriter,
    c: &mut Context,
) -> WriteResult {
    if f.entry.is_none() {
        return derr!((
            fname.0.loc,
            "the query attribute only works on public entry functions"
        ));
    }

    // validate that the function has:
    // move_to<X>(..., x) as last statement

    match &f.body.value {
        FunctionBody_::Native => {
            derr!((
                fname.0.loc,
                "the query attribute can only be used on user-defined entry functions"
            ))
        }
        FunctionBody_::Defined { locals: _, body } => {
            if body.is_empty() {
                return derr!((f.body.loc, "the query attribute can only be used on entry functions with a move_to<X>(signer, x); as the final statement"));
            }
            let last_stmt = body.get(body.len() - 1).unwrap();
            let err  = derr!((last_stmt.loc, "the query attribute can only be used on entry functions with a move_to<X>(signer, x); as the final statement"));
            match &last_stmt.value {
                Statement_::Command(command) => match &command.value {
                    Command_::Return { from_user: _, exp } => match &exp.exp.value {
                        UnannotatedExp_::Builtin(builtin_f, _) => match &builtin_f.value {
                            BuiltinFunction_::MoveTo(base) => {
                                write_query_function(fname, f, base, w, c)?;
                                c.add_query(&c.current_module.unwrap(), fname, f);
                                Ok(())
                            }
                            _ => err,
                        },
                        _ => err,
                    },
                    _ => err,
                },
                _ => err,
            }
        }
    }
}

pub fn handle_function_directives(
    fname: &FunctionName,
    f: &Function,
    w: &mut TsgenWriter,
    c: &mut Context,
) -> WriteResult {
    let attrs = &f.attributes;
    for (name, attr) in attrs.key_cloned_iter() {
        match name.to_string().as_str() {
            "cmd" => match &attr.value {
                Attribute_::Parameterized(_, inner_attrs) => {
                    w.new_line();
                    handle_function_cmd_directive(fname, f, Some(inner_attrs), w, c)?;
                }
                Attribute_::Name(_) => {
                    w.new_line();
                    handle_function_cmd_directive(fname, f, None, w, c)?;
                }
                Attribute_::Assigned(_, _) => {
                    return derr!((attr.loc, "the 'cmd' attribute cannot be assigned"))
                }
            },
            "query" => match &attr.value {
                Attribute_::Name(_) => {
                    w.new_line();
                    handle_function_query_directive(fname, f, w, c)?;
                }
                _ => return derr!((attr.loc, "the 'query' attribute has no parameters")),
            },
            "method" => {
                return derr!((
                    attr.loc,
                    "the 'method' attribute can only be used on structs"
                ))
            }
            _ => (),
        }
    }
    Ok(())
}

impl AstTsPrinter for (FunctionName, &Function) {
    const CTOR_NAME: &'static str = "FunctionDef";
    fn write_ts(&self, w: &mut TsgenWriter, c: &mut Context) -> WriteResult {
        let (name, func) = self;
        let is_entry = func.entry.is_some();
        if c.config.test {
            let is_test = check_test(name, func, c)?;
            if is_test {
                w.writeln("// #[test]");
            }
        }
        // yep, regardless of visibility, we always export it
        let async_modifier = if c.is_async() { "async " } else { "" };
        w.writeln(format!(
            "export {}function {}_ (",
            async_modifier,
            rename(name)
        ));
        // write parameters
        write_parameters(&func.signature, w, c, false, false)?;
        // cache & typeTags
        w.writeln("  $c: AptosDataCache,");
        let num_tparams = func.signature.type_parameters.len();
        let tpnames = if num_tparams == 0 {
            "".to_string()
        } else {
            func.signature
                .type_parameters
                .iter()
                .map(|tp| tp.user_specified_name.to_string())
                .join(", ")
        };
        if num_tparams > 0 {
            w.writeln(format!("  $p: TypeTag[], /* <{}>*/", tpnames));
        }
        // marks returnType or void
        w.write("): ");
        let ret_type_str = type_to_tstype(&func.signature.return_type, c)?;
        if c.is_async() {
            w.write(format!("Promise<{}>", ret_type_str));
        } else {
            w.write(ret_type_str);
        }
        w.write(" ");

        // set current_function_signature as we enter body
        c.current_function_signature = Some(func.signature.clone());
        // add parameters to local frame
        let mut param_names = BTreeSet::new();
        for (name, _) in func.signature.parameters.iter() {
            param_names.insert(name.to_string());
        }
        match &func.body.value {
            FunctionBody_::Native => {
                let mident = c.current_module.unwrap();
                let native_name = format!(
                    "return $.{}_{}_{}",
                    format_address(mident.value.address),
                    mident.value.module,
                    name
                );
                let args = func
                    .signature
                    .parameters
                    .iter()
                    .map(|(n, _)| rename(&n.to_string()))
                    .join(", ");
                let args_comma = format!("{}{}", args, if args.is_empty() { "" } else { ", " });
                let comma_tags = format!(
                    "{}{}",
                    if num_tparams == 0 { "" } else { ", " },
                    if num_tparams == 0 {
                        "".to_string()
                    } else {
                        format!(
                            "[{}]",
                            (0..num_tparams)
                                .into_iter()
                                .map(|u| format!("$p[{}]", u))
                                .join(", ")
                        )
                    }
                );
                w.short_block(|w| {
                    w.writeln(format!("{}({}$c{});", native_name, args_comma, comma_tags));
                    Ok(())
                })?;
            }
            FunctionBody_::Defined { locals, body } => {
                let new_vars = locals
                    .key_cloned_iter()
                    .map(|(name, _)| name)
                    .filter(|name| !param_names.contains(&name.to_string()))
                    .collect::<Vec<_>>();
                write_func_body(body, &new_vars, w, c)?;
            }
        }
        w.new_line();

        if is_entry && script_function_has_valid_parameter(&func.signature) {
            // TODO
            // uses entry-func signature, which returns TransactionInfo{toPayload(), send(),
            // sendAndWait()}
            w.new_line();
            // yep, regardless of visibility, we always export it
            w.writeln(format!("export function buildPayload_{} (", name));
            // write parameters
            write_parameters(&func.signature, w, c, true, false)?;
            // typeTags
            if num_tparams > 0 {
                w.writeln(format!("  $p: TypeTag[], /* <{}>*/", tpnames));
            }
            // marks returnType or void
            w.write(") ");
            // body:
            let params_no_signers = func
                .signature
                .parameters
                .iter()
                .filter(|(_n, ty)| !is_type_signer(ty))
                .collect::<Vec<_>>();

            w.short_block(|w| {
                let mident = c.current_module.unwrap();
                let address = format_address_hex(mident.value.address);
                if num_tparams > 0 {
                    w.writeln("const typeParamStrings = $p.map(t=>$.getTypeTagFullname(t));");
                } else {
                    w.writeln("const typeParamStrings = [] as string[];");
                }
                w.writeln("return $.buildPayload(");
                // function_name
                w.writeln(format!(
                    "  \"{}::{}::{}\",",
                    address, mident.value.module, name
                ));
                // type arguments
                w.writeln("  typeParamStrings,");
                // arguments
                if params_no_signers.is_empty() {
                    w.writeln("  []");
                } else {
                    w.writeln("  [");
                    for (pname, ptype) in params_no_signers.iter() {
                        w.writeln(format!(
                            "    {},",
                            get_ts_handler_for_script_function_param(pname, ptype)?,
                        ));
                    }
                    w.writeln("  ]");
                }
                w.writeln(");");
                Ok(())
            })?;
            w.new_line();
        }

        handle_function_directives(name, func, w, c)?;

        c.current_function_signature = None;

        Ok(())
    }
}

pub fn extract_builtin_from_base_type(
    ty: &BaseType,
) -> Result<(&BuiltinTypeName_, &Vec<BaseType>), bool> {
    if let BaseType_::Apply(_, typename, ty_args) = &ty.value {
        if let TypeName_::Builtin(builtin) = &typename.value {
            return Ok((&builtin.value, ty_args));
        }
    }
    Err(false)
}

pub fn extract_builtin_type(ty: &SingleType) -> Result<(&BuiltinTypeName_, &Vec<BaseType>), bool> {
    match &ty.value {
        SingleType_::Base(base_ty) => extract_builtin_from_base_type(base_ty),
        SingleType_::Ref(_, base_ty) => extract_builtin_from_base_type(base_ty),
    }
}

pub fn script_function_has_valid_parameter(sig: &FunctionSignature) -> bool {
    for (var, ty) in sig.parameters.iter() {
        if is_type_signer(ty) {
            continue;
        }
        let ts_handler = get_ts_handler_for_script_function_param(var, ty);
        if ts_handler.is_err() {
            return false;
        }
    }
    true
}

pub fn get_ts_handler_for_script_function_param(name: &Var, ty: &SingleType) -> TermResult {
    let name = rename(name);
    if let Ok((builtin, ty_args)) = extract_builtin_type(ty) {
        match builtin {
            BuiltinTypeName_::Bool
            | BuiltinTypeName_::Address
            | BuiltinTypeName_::U8
            | BuiltinTypeName_::U64
            | BuiltinTypeName_::U128 => Ok(format!("$.payloadArg({})", name)),
            BuiltinTypeName_::Signer => unreachable!(),
            BuiltinTypeName_::Vector => {
                // handle vector
                assert!(ty_args.len() == 1);
                if let Ok((inner_builtin, inner_ty_args)) =
                    extract_builtin_from_base_type(&ty_args[0])
                {
                    match inner_builtin {
                        BuiltinTypeName_::U8 => Ok(format!("$.u8ArrayArg({})", name)),
                        BuiltinTypeName_::Bool
                        | BuiltinTypeName_::Address
                        | BuiltinTypeName_::U64
                        | BuiltinTypeName_::U128 => {
                            Ok(format!("{}.map(element => $.payloadArg(element))", name))
                        }
                        BuiltinTypeName_::Signer => unreachable!(),
                        BuiltinTypeName_::Vector => {
                            assert!(inner_ty_args.len() == 1);
                            let inner_map = get_ts_handler_for_vector_in_vector(&inner_ty_args[0])?;
                            Ok(format!("{}.map({})", name, inner_map))
                        }
                    }
                } else {
                    derr!((
                        ty.loc,
                        "This vector type is not supported as parameter of a script function"
                    ))
                }
            }
        }
    } else {
        derr!((
            ty.loc,
            "This type is not supported as parameter of script function"
        ))
    }
}

pub fn get_ts_handler_for_vector_in_vector(inner_ty: &BaseType) -> TermResult {
    if let Ok((builtin, inner_ty_args)) = extract_builtin_from_base_type(inner_ty) {
        match builtin {
            BuiltinTypeName_::U8 => Ok(format!("array => $.u8ArrayArg(array)")),
            BuiltinTypeName_::Bool
            | BuiltinTypeName_::Address
            | BuiltinTypeName_::U64
            | BuiltinTypeName_::U128 => {
                Ok("array => array.map(ele => $.payloadArg(ele))".to_string())
            }
            BuiltinTypeName_::Signer => unreachable!(),
            BuiltinTypeName_::Vector => {
                assert!(inner_ty_args.len() == 1);
                let inner_map = get_ts_handler_for_vector_in_vector(&inner_ty_args[0])?;
                Ok(format!("array => array.map({})", inner_map))
            }
        }
    } else {
        derr!((inner_ty.loc, "Unsupported vector-in-vector type"))
    }
}

pub fn is_base_type_signer(ty: &BaseType) -> bool {
    match &ty.value {
        BaseType_::Apply(_, typename, _) => match &typename.value {
            TypeName_::Builtin(builtin) => {
                return builtin.value == BuiltinTypeName_::Signer;
            }
            _ => false,
        },
        _ => false,
    }
}

pub fn is_type_signer(ty: &SingleType) -> bool {
    // includes signer or &signer
    match &ty.value {
        SingleType_::Base(base_ty) => is_base_type_signer(base_ty),
        SingleType_::Ref(_, base_ty) => is_base_type_signer(base_ty),
    }
}

pub fn ts_constant_type(ty: &BaseType, c: &mut Context) -> TermResult {
    // only builtin types allowed as top-level constants
    match &ty.value {
        BaseType_::Apply(_, type_name, type_args) => match &type_name.value {
            TypeName_::Builtin(builtin_type_name) => match builtin_type_name.value {
                BuiltinTypeName_::Vector => {
                    Ok(format!("{}[]", ts_constant_type(&type_args[0], c)?))
                }
                _ => builtin_type_name.term(c),
            },
            _ => unreachable!("Only builtin types supported as constants"),
        },
        _ => unreachable!("Only builtin types supported as constants"),
    }
}

pub fn is_empty_block(block: &Block) -> bool {
    if block.is_empty() {
        return true;
    } else if block.len() == 1 {
        return match &block[0].value {
            Statement_::Command(cmd) => match &cmd.value {
                Command_::IgnoreAndPop { pop_num: _, exp } => is_exp_unit(exp),
                _ => false,
            },
            _ => false,
        };
    }
    false
}

pub fn identify_declared_vars_in_lvalue(lvalue: &LValue, declared: &mut BTreeSet<String>) {
    use LValue_ as L;
    match &lvalue.value {
        L::Ignore => (),
        L::Var(_, _) => {
            //declared.insert(var.to_string());
        }
        L::Unpack(_, _, fields) => {
            for (_, lvalue) in fields.iter() {
                if let LValue_::Var(var, _) = &lvalue.value {
                    declared.insert(var.to_string());
                } else {
                    identify_declared_vars_in_lvalue(lvalue, declared);
                }
            }
        }
    }
}

pub fn identify_declared_vars_in_cmd(cmd: &Command, declared: &mut BTreeSet<String>) {
    use Command_ as C;
    match &cmd.value {
        C::Assign(lvalues, _) => lvalues.iter().for_each(|lvalue| {
            identify_declared_vars_in_lvalue(lvalue, declared);
        }),
        _ => (),
    }
}

pub fn identify_declared_vars_in_stmt(stmt: &Statement, declared: &mut BTreeSet<String>) {
    use Statement_ as S;
    match &stmt.value {
        S::Command(cmd) => identify_declared_vars_in_cmd(cmd, declared),
        S::IfElse {
            cond: _,
            if_block,
            else_block,
        } => {
            identify_declared_vars_in_block(if_block, declared);
            identify_declared_vars_in_block(else_block, declared);
        }
        S::While { cond, block } => {
            let (pre_block, _cond_exp) = cond;
            identify_declared_vars_in_block(block, declared);
            identify_declared_vars_in_block(pre_block, declared);
        }
        S::Loop {
            has_break: _,
            block,
        } => identify_declared_vars_in_block(block, declared),
    };
}

pub fn identify_declared_vars_in_block(block: &Block, undeclared: &mut BTreeSet<String>) {
    for stmt in block.iter() {
        identify_declared_vars_in_stmt(stmt, undeclared);
    }
}

pub fn write_func_body(
    block: &Block,
    new_vars: &Vec<Var>,
    w: &mut TsgenWriter,
    c: &mut Context,
) -> WriteResult {
    w.writeln("{");
    w.increase_indent();

    let mut declared_vars = BTreeSet::<String>::new();
    identify_declared_vars_in_block(block, &mut declared_vars);

    let undeclared = new_vars
        .iter()
        .filter(|var| !declared_vars.contains(&var.to_string()))
        .collect::<Vec<_>>();

    if undeclared.len() > 0 {
        w.writeln(format!(
            "let {};",
            undeclared
                .into_iter()
                .map(|v| rename(&v.to_string()))
                .join(", ")
        ));
    }

    for stmt in block.iter() {
        stmt.write_ts(w, c)?;
    }

    w.decrease_indent();
    w.writeln("}");

    Ok(())
}

impl AstTsPrinter for Block {
    const CTOR_NAME: &'static str = "Block";
    fn write_ts(&self, w: &mut TsgenWriter, c: &mut Context) -> WriteResult {
        w.writeln("{");
        w.increase_indent();

        for stmt in self.iter() {
            stmt.write_ts(w, c)?;
        }

        w.decrease_indent();
        w.writeln("}");

        Ok(())
    }
}

impl AstTsPrinter for Statement {
    const CTOR_NAME: &'static str = "Statement";
    fn write_ts(&self, w: &mut TsgenWriter, c: &mut Context) -> WriteResult {
        use Statement_ as S;
        // some value-yielding Block can be formatted as lambdas, and need statements to be
        // presented in the form of ts_term
        match &self.value {
            S::Command(cmd) => cmd.write_ts(w, c),
            S::IfElse {
                cond,
                if_block,
                else_block,
            } => {
                // FIXME in case it's a single statement, need indentation here
                if !is_empty_block(if_block) {
                    // if-block is non-empty
                    w.write(format!("if ({}) ", cond.term(c)?));
                    if_block.write_ts(w, c)?;
                    if else_block.len() > 0 {
                        w.write("else");
                        else_block.write_ts(w, c)?;
                    }
                } else {
                    // if-block is empty, negate condition and output else block only
                    w.write(format!("if (!{}) ", cond.term(c)?));
                    else_block.write_ts(w, c)?;
                }
                Ok(())
            }
            S::While { cond, block } => {
                let (pre_block, cond_exp) = cond;
                // FIXME need to handle the empty case
                let has_pre_block = pre_block.len() > 0;
                w.write(format!(
                    "while ({}) ",
                    if has_pre_block {
                        "true".to_string()
                    } else {
                        cond_exp.term(c)?
                    }
                ));
                w.short_block(|w| {
                    if has_pre_block {
                        pre_block.write_ts(w, c)?;
                        w.writeln(format!("if (!({})) break;", cond_exp.term(c)?));
                    }
                    block.write_ts(w, c)?;
                    Ok(())
                })?;
                Ok(())
            }
            S::Loop {
                has_break: _,
                block,
            } => {
                w.write("while (true) ");
                block.write_ts(w, c)
            }
        }
    }
}

pub fn is_exp_unit(exp: &Exp) -> bool {
    matches!(exp.exp.value, UnannotatedExp_::Unit { case: _ })
}

impl AstTsPrinter for Command {
    const CTOR_NAME: &'static str = "Command";

    fn write_ts(&self, w: &mut TsgenWriter, c: &mut Context) -> WriteResult {
        use Command_ as C;
        match &self.value {
            C::Assign(lvalues, rhs) => {
                if is_empty_lvalue_list(lvalues) {
                    w.writeln(format!("{};", rhs.term(c)?));
                } else {
                    if lvalues.len() == 1 && matches!(lvalues[0].value, LValue_::Unpack(_, _, _)) {
                        w.write("let ");
                    }
                    // using write_ts instead of term to allow prettier printing in case we ever
                    // want to do that
                    lvalues.write_ts(w, c)?;
                    w.write(" = ");
                    w.write(rhs.term(c)?);
                    w.writeln(";");
                }
            }
            C::Mutate(lhs, rhs) => match &lhs.exp.value {
                // DerefAssign
                UnannotatedExp_::Borrow(_, _, _) => {
                    w.writeln(format!("{} = {};", lhs.term(c)?, rhs.term(c)?));
                }
                UnannotatedExp_::Dereference(_) => {
                    return derr!((lhs.exp.loc, "Dereference in Mutate not implemented yet"));
                }
                _ => {
                    w.writeln(format!("$.set({}, {});", lhs.term(c)?, rhs.term(c)?));
                }
            },
            C::Abort(e) => w.writeln(format!("throw $.abortCode({});", e.term(c)?)),
            C::Return { from_user: _, exp } => {
                if is_exp_unit(exp) {
                    w.writeln("return;");
                } else {
                    w.writeln(format!("return {};", exp.term(c)?));
                }
            }
            C::Break => w.writeln("break;"),
            C::Continue => w.writeln("continue;"),
            C::IgnoreAndPop { pop_num: _, exp } => {
                if is_exp_unit(exp) {
                    // do nothing..
                    // w.writeln("/*PopAndIgnore*/");
                } else {
                    w.writeln(format!("{};", exp.term(c)?));
                }
            }
            _ => {
                return derr!((self.loc, "Unsupported Command (Jump)"));
            }
        }
        Ok(())
    }
}
