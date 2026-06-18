#![allow(unused_variables)]
use std::{cell::RefCell, ops::{Deref, Range}};

use crate::CillyBackend;
use cilly::{Assembly, CILNode, MethodRef, Type};

use rustc_abi::*;
use rustc_ast::{InlineAsmOptions, InlineAsmTemplatePiece};
use rustc_codegen_ssa::{
    common::*,
    mir::{debuginfo::*, operand::*, place::*},
    traits::*,
    *,
};
use rustc_const_eval::interpret::Scalar;
use rustc_data_structures::fx::FxHashMap;
use rustc_middle::mir::coverage::CoverageKind;
use rustc_middle::mir::interpret::ConstAllocation;
use rustc_middle::ty::layout::FnAbiError;
use rustc_middle::ty::layout::FnAbiOfHelpers;
use rustc_middle::ty::layout::FnAbiRequest;
use rustc_middle::ty::layout::LayoutError;
use rustc_middle::ty::layout::LayoutOfHelpers;
use rustc_middle::ty::layout::MaybeResult;
use rustc_middle::ty::layout::TyAndLayout as TyAndLayoutT;
use rustc_middle::{
    middle::codegen_fn_attrs::CodegenFnAttrs,
    mir::{mono::Linkage, Body},
    ty::*,
};
use rustc_session::Session;
use rustc_span::{def_id::DefId, *};
use rustc_target::callconv::*;
pub struct CodegenCx<'tcx> {
     tcx: TyCtxt<'tcx>,
     asm:Assembly
}
impl<'tcx> CodegenCx<'tcx>{
    pub fn new( tcx: TyCtxt<'tcx>)->Self{
        Self { tcx,asm:Assembly::default() }
    }
}
pub struct Builder<'tcx> {
    tcx: TyCtxt<'tcx>,
}
impl<'tcx, 'a> Deref for Builder<'tcx> {
    fn deref(&self) -> &Self::Target {
        todo!()
    }

    type Target = CodegenCx<'tcx>;
}
impl<'tcx> Builder<'tcx> {
    pub(crate) fn new(cx: CillyBackend, tcx: TyCtxt<'tcx>) -> Self {
        Self { tcx }
    }
}
impl<'tcx> TypeMembershipCodegenMethods<'tcx> for CodegenCx<'tcx> {}
impl<'tcx> StaticBuilderMethods for Builder<'tcx> {
    // Required method
    fn get_static(&mut self, def_id: DefId) -> Self::Value {
        todo!()
    }
}
impl<'tcx> BackendTypes for Builder<'tcx> {
    type Value = cilly::v2::Interned<CILNode>;
    type Metadata = ();
    type Function = cilly::v2::Interned<MethodRef>;
    type BasicBlock = u32;
    type Type = cilly::v2::Interned<Type>;
    type Funclet = ();
    type DIScope = ();
    type DILocation = ();
    type DIVariable = ();
}
impl<'tcx> BackendTypes for CodegenCx<'tcx> {
    type Value = cilly::v2::Interned<CILNode>;
    type Metadata = ();
    type Function = cilly::v2::Interned<MethodRef>;
    type BasicBlock = u32;
    type Type = cilly::v2::Interned<Type>;
    type Funclet = ();
    type DIScope = ();
    type DILocation = ();
    type DIVariable = ();
}
impl<'tcx> rustc_middle::ty::layout::HasTypingEnv<'tcx> for Builder<'tcx> {
    fn typing_env(&self) -> TypingEnv<'tcx> {
        todo!()
    }
}
impl<'tcx> rustc_abi::HasDataLayout for Builder<'tcx> {
    // Required method
    fn data_layout(&self) -> &TargetDataLayout {
        todo!()
    }
}

impl<'tcx> rustc_middle::ty::layout::HasTyCtxt<'tcx> for Builder<'tcx> {
    fn tcx(&self) -> TyCtxt<'tcx> {
        //self.tcx
        todo!()
    }
}
impl<'tcx> LayoutOfHelpers<'tcx> for Builder<'tcx> {
    fn handle_layout_err(
        &self,
        err: LayoutError<'tcx>,
        span: Span,
        ty: Ty<'tcx>,
    ) -> <Self::LayoutOfResult as MaybeResult<TyAndLayout<'tcx, Ty<'tcx>>>>::Error {
        todo!()
    }
}
impl<'tcx> FnAbiOfHelpers<'tcx> for Builder<'tcx> {
    fn handle_fn_abi_err(
        &self,
        err: FnAbiError<'tcx>,
        span: Span,
        fn_abi_request: FnAbiRequest<'tcx>,
    ) -> <Self::FnAbiOfResult as MaybeResult<&'tcx FnAbi<'tcx, Ty<'tcx>>>>::Error {
        todo!()
    }
}
impl<'tcx> rustc_middle::ty::layout::HasTypingEnv<'tcx> for CodegenCx<'tcx> {
    fn typing_env(&self) -> TypingEnv<'tcx> {
        todo!()
    }
}
impl<'tcx> rustc_abi::HasDataLayout for CodegenCx<'tcx> {
    // Required method
    fn data_layout(&self) -> &TargetDataLayout {
        todo!()
    }
}

impl<'tcx> rustc_middle::ty::layout::HasTyCtxt<'tcx> for CodegenCx<'tcx> {
    fn tcx(&self) -> TyCtxt<'tcx> {
        self.tcx
    }
}
impl<'tcx> LayoutOfHelpers<'tcx> for CodegenCx<'tcx> {
    fn handle_layout_err(
        &self,
        err: LayoutError<'tcx>,
        span: Span,
        ty: Ty<'tcx>,
    ) -> <Self::LayoutOfResult as MaybeResult<TyAndLayout<'tcx, Ty<'tcx>>>>::Error {
        todo!()
    }
}
impl<'tcx> FnAbiOfHelpers<'tcx> for CodegenCx<'tcx> {
    fn handle_fn_abi_err(
        &self,
        err: FnAbiError<'tcx>,
        span: Span,
        fn_abi_request: FnAbiRequest<'tcx>,
    ) -> <Self::FnAbiOfResult as MaybeResult<&'tcx FnAbi<'tcx, Ty<'tcx>>>>::Error {
        todo!()
    }
}
impl<'tcx> CoverageInfoBuilderMethods<'tcx> for Builder<'tcx> {
    fn add_coverage(&mut self, instance: Instance<'tcx>, kind: &CoverageKind) {}
}
impl<'tcx> DebugInfoBuilderMethods for Builder<'tcx> {
    fn dbg_var_addr(
        &mut self,
        dbg_var: Self::DIVariable,
        dbg_loc: Self::DILocation,
        variable_alloca: Self::Value,
        direct_offset: Size,
        indirect_offsets: &[Size],
        fragment: Option<Range<Size>>,
    ){}
    fn set_dbg_loc(&mut self, dbg_loc: Self::DILocation){}
    fn clear_dbg_loc(&mut self){}
    fn insert_reference_to_gdb_debug_scripts_section_global(&mut self){}
    fn set_var_name(&mut self, value: Self::Value, name: &str){}
}
impl<'tcx> IntrinsicCallBuilderMethods<'tcx> for Builder<'tcx> {
    fn codegen_intrinsic_call(
        &mut self,
        instance: Instance<'tcx>,
        args: &[OperandRef<'tcx, Self::Value>],
        result_dest: PlaceRef<'tcx, Self::Value>,
        span: Span,
    ) -> Result<(), Instance<'tcx>>{todo!()}
    fn abort(&mut self){todo!()}
    fn assume(&mut self, val: Self::Value){todo!()}
    fn expect(&mut self, cond: Self::Value, expected: bool) -> Self::Value{todo!()}
    fn type_checked_load(
        &mut self,
        llvtable: Self::Value,
        vtable_byte_offset: u64,
        typeid: Self::Metadata,
    ) -> Self::Value{todo!()}
    fn va_start(&mut self, val: Self::Value) -> Self::Value{todo!()}
    fn va_end(&mut self, val: Self::Value) -> Self::Value{todo!()}
}
impl<'tcx> AbiBuilderMethods for Builder<'tcx> {
    fn get_param(&mut self, index: usize) -> Self::Value{
        todo!()
    }
}
impl<'tcx> AsmBuilderMethods<'tcx>  for Builder<'tcx> {
      fn codegen_inline_asm(
        &mut self,
        template: &[InlineAsmTemplatePiece],
        operands: &[InlineAsmOperandRef<'tcx, Self>],
        options: InlineAsmOptions,
        line_spans: &[Span],
        instance: Instance<'_>,
        dest: Option<Self::BasicBlock>,
        catch_funclet: Option<(Self::BasicBlock, Option<&Self::Funclet>)>,
    ){
        todo!()
    }
}
impl<'tcx> ArgAbiBuilderMethods<'tcx> for Builder<'tcx> {
    fn store_fn_arg(
        &mut self,
        arg_abi: &ArgAbi<'tcx, Ty<'tcx>>,
        idx: &mut usize,
        dst: PlaceRef<'tcx, Self::Value>,
    ){todo!()}
    fn store_arg(
        &mut self,
        arg_abi: &ArgAbi<'tcx, Ty<'tcx>>,
        val: Self::Value,
        dst: PlaceRef<'tcx, Self::Value>,
    ){todo!()}
}

impl<'tcx> BaseTypeCodegenMethods for CodegenCx<'tcx> {
    fn type_i8(&self) -> Self::Type {
        todo!()
    }
    fn type_i16(&self) -> Self::Type {
        todo!()
    }
    fn type_i32(&self) -> Self::Type {
        todo!()
    }
    fn type_i64(&self) -> Self::Type {
        todo!()
    }
    fn type_i128(&self) -> Self::Type {
        todo!()
    }
    fn type_isize(&self) -> Self::Type {
        todo!()
    }
    fn type_f16(&self) -> Self::Type {
        todo!()
    }
    fn type_f32(&self) -> Self::Type {
        todo!()
    }
    fn type_f64(&self) -> Self::Type {
        todo!()
    }
    fn type_f128(&self) -> Self::Type {
        todo!()
    }
    fn type_array(&self, ty: Self::Type, len: u64) -> Self::Type {
        todo!()
    }
    fn type_func(&self, args: &[Self::Type], ret: Self::Type) -> Self::Type {
        todo!()
    }
    fn type_kind(&self, ty: Self::Type) -> TypeKind {
        todo!()
    }
    fn type_ptr(&self) -> Self::Type {
        todo!()
    }
    fn type_ptr_ext(&self, address_space: AddressSpace) -> Self::Type {
        todo!()
    }
    fn element_type(&self, ty: Self::Type) -> Self::Type {
        todo!()
    }
    fn vector_length(&self, ty: Self::Type) -> usize {
        todo!()
    }
    fn float_width(&self, ty: Self::Type) -> usize {
        todo!()
    }
    fn int_width(&self, ty: Self::Type) -> u64 {
        todo!()
    }
    fn val_ty(&self, v: Self::Value) -> Self::Type {
        todo!()
    }
}
impl<'tcx> ConstCodegenMethods for CodegenCx<'tcx> {
    fn const_null(&self, t: Self::Type) -> Self::Value {
        todo!()
    }
    fn const_undef(&self, t: Self::Type) -> Self::Value {
        todo!()
    }
    fn const_poison(&self, t: Self::Type) -> Self::Value {
        todo!()
    }
    fn const_bool(&self, val: bool) -> Self::Value {
        todo!()
    }
    fn const_i8(&self, i: i8) -> Self::Value {
        todo!()
    }
    fn const_i16(&self, i: i16) -> Self::Value {
        todo!()
    }
    fn const_i32(&self, i: i32) -> Self::Value {
        todo!()
    }
    fn const_int(&self, t: Self::Type, i: i64) -> Self::Value {
        todo!()
    }
    fn const_u8(&self, i: u8) -> Self::Value {
        todo!()
    }
    fn const_u32(&self, i: u32) -> Self::Value {
        todo!()
    }
    fn const_u64(&self, i: u64) -> Self::Value {
        todo!()
    }
    fn const_u128(&self, i: u128) -> Self::Value {
        todo!()
    }
    fn const_usize(&self, i: u64) -> Self::Value {
        todo!()
    }
    fn const_uint(&self, t: Self::Type, i: u64) -> Self::Value {
        todo!()
    }
    fn const_uint_big(&self, t: Self::Type, u: u128) -> Self::Value {
        todo!()
    }
    fn const_real(&self, t: Self::Type, val: f64) -> Self::Value {
        todo!()
    }
    fn const_str(&self, s: &str) -> (Self::Value, Self::Value) {
        todo!()
    }
    fn const_struct(&self, elts: &[Self::Value], packed: bool) -> Self::Value {
        todo!()
    }
    fn const_vector(&self, elts: &[Self::Value]) -> Self::Value {
        todo!()
    }
    fn const_to_opt_uint(&self, v: Self::Value) -> Option<u64> {
        todo!()
    }
    fn const_to_opt_u128(&self, v: Self::Value, sign_ext: bool) -> Option<u128> {
        todo!()
    }
    fn const_data_from_alloc(&self, alloc: ConstAllocation<'_>) -> Self::Value {
        todo!()
    }
    fn scalar_to_backend(
        &self,
        cv: Scalar,
        layout: rustc_abi::Scalar,
        llty: Self::Type,
    ) -> Self::Value {
        todo!()
    }
    fn const_ptr_byte_offset(&self, val: Self::Value, offset: Size) -> Self::Value {
        todo!()
    }
}
impl<'tcx> LayoutTypeCodegenMethods<'tcx> for CodegenCx<'tcx> {
    fn backend_type(&self, layout: TyAndLayoutT<'tcx>) -> Self::Type {
        todo!()
    }
    fn cast_backend_type(&self, ty: &CastTarget) -> Self::Type {
        todo!()
    }
    fn fn_decl_backend_type(&self, fn_abi: &FnAbi<'tcx, Ty<'tcx>>) -> Self::Type {
        todo!()
    }
    fn fn_ptr_backend_type(&self, fn_abi: &FnAbi<'tcx, Ty<'tcx>>) -> Self::Type {
        todo!()
    }
    fn reg_backend_type(&self, ty: &Reg) -> Self::Type {
        todo!()
    }
    fn immediate_backend_type(&self, layout: TyAndLayoutT<'tcx>) -> Self::Type {
        todo!()
    }
    fn is_backend_immediate(&self, layout: TyAndLayoutT<'tcx>) -> bool {
        todo!()
    }
    fn is_backend_scalar_pair(&self, layout: TyAndLayoutT<'tcx>) -> bool {
        todo!()
    }
    fn scalar_pair_element_backend_type(
        &self,
        layout: TyAndLayoutT<'tcx>,
        index: usize,
        immediate: bool,
    ) -> Self::Type {
        todo!()
    }
}

impl<'tcx> StaticCodegenMethods for CodegenCx<'tcx> {
    fn static_addr_of(&self, cv: Self::Value, align: Align, kind: Option<&str>) -> Self::Value {
        todo!()
    }
    fn codegen_static(&mut self, def_id: DefId) {
        todo!()
    }
}
impl<'tcx> MiscCodegenMethods<'tcx> for CodegenCx<'tcx> {
    fn vtables(
        &self,
    ) -> &RefCell<FxHashMap<(Ty<'tcx>, Option<ExistentialTraitRef<'tcx>>), Self::Value>> {
        todo!()
    }
    fn get_fn(&self, instance: Instance<'tcx>) -> Self::Function {
        todo!()
    }
    fn get_fn_addr(&self, instance: Instance<'tcx>) -> Self::Value {
        todo!()
    }
    fn eh_personality(&self) -> Self::Function {
        todo!()
    }
    fn sess(&self) -> &Session {
        todo!()
    }
    fn set_frame_pointer_type(&self, llfn: Self::Function) {
        todo!()
    }
    fn apply_target_cpu_attr(&self, llfn: Self::Function) {
        todo!()
    }
    fn declare_c_main(&self, fn_type: Self::Type) -> Option<Self::Function> {
        todo!()
    }
}
impl<'tcx> PreDefineCodegenMethods<'tcx> for CodegenCx<'tcx> {
    fn predefine_static(
        &mut self,
        def_id: DefId,
        linkage: Linkage,
        visibility: rustc_middle::mono::Visibility,
        symbol_name: &str,
    ) {
        todo!()
    }
    fn predefine_fn(
        &mut self,
        instance: Instance<'tcx>,
        linkage: Linkage,
        visibility: rustc_middle::mono::Visibility,
        symbol_name: &str,
    ) {
        // TODO: Should I do something here?
    }
}
impl<'tcx> AsmCodegenMethods<'tcx> for CodegenCx<'tcx> {
    fn codegen_global_asm(
        &mut self,
        template: &[InlineAsmTemplatePiece],
        operands: &[GlobalAsmOperandRef<'tcx>],
        options: InlineAsmOptions,
        line_spans: &[Span],
    ) {
        todo!()
    }
    fn mangled_name(&self, instance: Instance<'tcx>) -> String {
        todo!()
    }
}
impl<'tcx> DebugInfoCodegenMethods<'tcx> for CodegenCx<'tcx> {
    fn create_vtable_debuginfo(
        &self,
        ty: Ty<'tcx>,
        trait_ref: Option<ExistentialTraitRef<'tcx>>,
        vtable: Self::Value,
    ) {
        todo!()
    }
    fn create_function_debug_context(
        &self,
        instance: Instance<'tcx>,
        fn_abi: &FnAbi<'tcx, Ty<'tcx>>,
        llfn: Self::Function,
        mir: &Body<'tcx>,
    ) -> Option<FunctionDebugContext<'tcx, Self::DIScope, Self::DILocation>> {
        todo!()
    }
    fn dbg_scope_fn(
        &self,
        instance: Instance<'tcx>,
        fn_abi: &FnAbi<'tcx, Ty<'tcx>>,
        maybe_definition_llfn: Option<Self::Function>,
    ) -> Self::DIScope {
        todo!()
    }
    fn dbg_loc(
        &self,
        scope: Self::DIScope,
        inlined_at: Option<Self::DILocation>,
        span: Span,
    ) -> Self::DILocation {
        todo!()
    }
    fn extend_scope_to_file(
        &self,
        scope_metadata: Self::DIScope,
        file: &SourceFile,
    ) -> Self::DIScope {
        todo!()
    }
    fn debuginfo_finalize(&self) {
        todo!()
    }
    fn create_dbg_var(
        &self,
        variable_name: Symbol,
        variable_type: Ty<'tcx>,
        scope_metadata: Self::DIScope,
        variable_kind: VariableKind,
        span: Span,
    ) -> Self::DIVariable {
        todo!()
    }
}

impl<'a, 'tcx> BuilderMethods<'a, 'tcx> for Builder<'tcx> {
    type CodegenCx = CodegenCx<'tcx>;
    fn build(cx: &'a Self::CodegenCx, llbb: Self::BasicBlock) -> Self {
        todo!()
    }
    fn cx(&self) -> &Self::CodegenCx {
        todo!()
    }
    fn llbb(&self) -> Self::BasicBlock {
        todo!()
    }
    fn set_span(&mut self, span: Span) {
        todo!()
    }
    fn append_block(cx: &'a Self::CodegenCx, llfn: Self::Function, name: &str) -> Self::BasicBlock {
        todo!()
    }
    fn append_sibling_block(&mut self, name: &str) -> Self::BasicBlock {
        todo!()
    }
    fn switch_to_block(&mut self, llbb: Self::BasicBlock) {
        todo!()
    }
    fn ret_void(&mut self) {
        todo!()
    }
    fn ret(&mut self, v: Self::Value) {
        todo!()
    }
    fn br(&mut self, dest: Self::BasicBlock) {
        todo!()
    }
    fn cond_br(
        &mut self,
        cond: Self::Value,
        then_llbb: Self::BasicBlock,
        else_llbb: Self::BasicBlock,
    ) {
        todo!()
    }
    fn switch(
        &mut self,
        v: Self::Value,
        else_llbb: Self::BasicBlock,
        cases: impl ExactSizeIterator<Item = (u128, Self::BasicBlock)>,
    ) {
        todo!()
    }
    fn invoke(
        &mut self,
        llty: Self::Type,
        fn_attrs: Option<&CodegenFnAttrs>,
        fn_abi: Option<&FnAbi<'tcx, Ty<'tcx>>>,
        llfn: Self::Value,
        args: &[Self::Value],
        then: Self::BasicBlock,
        catch: Self::BasicBlock,
        funclet: Option<&Self::Funclet>,
        instance: Option<Instance<'tcx>>,
    ) -> Self::Value {
        todo!()
    }
    fn unreachable(&mut self) {
        todo!()
    }
    fn add(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn fadd(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn fadd_fast(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn fadd_algebraic(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn sub(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn fsub(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn fsub_fast(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn fsub_algebraic(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn mul(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn fmul(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn fmul_fast(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn fmul_algebraic(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn udiv(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn exactudiv(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn sdiv(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn exactsdiv(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn fdiv(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn fdiv_fast(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn fdiv_algebraic(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn urem(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn srem(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn frem(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn frem_fast(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn frem_algebraic(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn shl(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn lshr(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn ashr(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn and(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn or(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn xor(&mut self, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn neg(&mut self, v: Self::Value) -> Self::Value {
        todo!()
    }
    fn fneg(&mut self, v: Self::Value) -> Self::Value {
        todo!()
    }
    fn not(&mut self, v: Self::Value) -> Self::Value {
        todo!()
    }
    fn checked_binop(
        &mut self,
        oop: OverflowOp,
        ty: Ty<'tcx>,
        lhs: Self::Value,
        rhs: Self::Value,
    ) -> (Self::Value, Self::Value) {
        todo!()
    }
    fn from_immediate(&mut self, val: Self::Value) -> Self::Value {
        todo!()
    }
    fn to_immediate_scalar(&mut self, val: Self::Value, scalar: rustc_abi::Scalar) -> Self::Value {
        todo!()
    }
    fn alloca(&mut self, size: Size, align: Align) -> Self::Value {
        todo!()
    }
    fn load(&mut self, ty: Self::Type, ptr: Self::Value, align: Align) -> Self::Value {
        todo!()
    }
    fn volatile_load(&mut self, ty: Self::Type, ptr: Self::Value) -> Self::Value {
        todo!()
    }
    fn atomic_load(
        &mut self,
        ty: Self::Type,
        ptr: Self::Value,
        order: AtomicOrdering,
        size: Size,
    ) -> Self::Value {
        todo!()
    }
    fn load_operand(
        &mut self,
        place: PlaceRef<'tcx, Self::Value>,
    ) -> OperandRef<'tcx, Self::Value> {
        todo!()
    }
    fn write_operand_repeatedly(
        &mut self,
        elem: OperandRef<'tcx, Self::Value>,
        count: u64,
        dest: PlaceRef<'tcx, Self::Value>,
    ) {
        todo!()
    }
    fn range_metadata(&mut self, load: Self::Value, range: WrappingRange) {
        todo!()
    }
    fn nonnull_metadata(&mut self, load: Self::Value) {
        todo!()
    }
    fn store(&mut self, val: Self::Value, ptr: Self::Value, align: Align) -> Self::Value {
        todo!()
    }
    fn store_with_flags(
        &mut self,
        val: Self::Value,
        ptr: Self::Value,
        align: Align,
        flags: MemFlags,
    ) -> Self::Value {
        todo!()
    }
    fn atomic_store(
        &mut self,
        val: Self::Value,
        ptr: Self::Value,
        order: AtomicOrdering,
        size: Size,
    ) {
        todo!()
    }
    fn gep(&mut self, ty: Self::Type, ptr: Self::Value, indices: &[Self::Value]) -> Self::Value {
        todo!()
    }
    fn inbounds_gep(
        &mut self,
        ty: Self::Type,
        ptr: Self::Value,
        indices: &[Self::Value],
    ) -> Self::Value {
        todo!()
    }
    fn trunc(&mut self, val: Self::Value, dest_ty: Self::Type) -> Self::Value {
        todo!()
    }
    fn sext(&mut self, val: Self::Value, dest_ty: Self::Type) -> Self::Value {
        todo!()
    }
    fn fptoui_sat(&mut self, val: Self::Value, dest_ty: Self::Type) -> Self::Value {
        todo!()
    }
    fn fptosi_sat(&mut self, val: Self::Value, dest_ty: Self::Type) -> Self::Value {
        todo!()
    }
    fn fptoui(&mut self, val: Self::Value, dest_ty: Self::Type) -> Self::Value {
        todo!()
    }
    fn fptosi(&mut self, val: Self::Value, dest_ty: Self::Type) -> Self::Value {
        todo!()
    }
    fn uitofp(&mut self, val: Self::Value, dest_ty: Self::Type) -> Self::Value {
        todo!()
    }
    fn sitofp(&mut self, val: Self::Value, dest_ty: Self::Type) -> Self::Value {
        todo!()
    }
    fn fptrunc(&mut self, val: Self::Value, dest_ty: Self::Type) -> Self::Value {
        todo!()
    }
    fn fpext(&mut self, val: Self::Value, dest_ty: Self::Type) -> Self::Value {
        todo!()
    }
    fn ptrtoint(&mut self, val: Self::Value, dest_ty: Self::Type) -> Self::Value {
        todo!()
    }
    fn inttoptr(&mut self, val: Self::Value, dest_ty: Self::Type) -> Self::Value {
        todo!()
    }
    fn bitcast(&mut self, val: Self::Value, dest_ty: Self::Type) -> Self::Value {
        todo!()
    }
    fn intcast(&mut self, val: Self::Value, dest_ty: Self::Type, is_signed: bool) -> Self::Value {
        todo!()
    }
    fn pointercast(&mut self, val: Self::Value, dest_ty: Self::Type) -> Self::Value {
        todo!()
    }
    fn icmp(&mut self, op: IntPredicate, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn fcmp(&mut self, op: RealPredicate, lhs: Self::Value, rhs: Self::Value) -> Self::Value {
        todo!()
    }
    fn memcpy(
        &mut self,
        dst: Self::Value,
        dst_align: Align,
        src: Self::Value,
        src_align: Align,
        size: Self::Value,
        flags: MemFlags,
    ) {
        todo!()
    }
    fn memmove(
        &mut self,
        dst: Self::Value,
        dst_align: Align,
        src: Self::Value,
        src_align: Align,
        size: Self::Value,
        flags: MemFlags,
    ) {
        todo!()
    }
    fn memset(
        &mut self,
        ptr: Self::Value,
        fill_byte: Self::Value,
        size: Self::Value,
        align: Align,
        flags: MemFlags,
    ) {
        todo!()
    }
    fn select(
        &mut self,
        cond: Self::Value,
        then_val: Self::Value,
        else_val: Self::Value,
    ) -> Self::Value {
        todo!()
    }
    fn va_arg(&mut self, list: Self::Value, ty: Self::Type) -> Self::Value {
        todo!()
    }
    fn extract_element(&mut self, vec: Self::Value, idx: Self::Value) -> Self::Value {
        todo!()
    }
    fn vector_splat(&mut self, num_elts: usize, elt: Self::Value) -> Self::Value {
        todo!()
    }
    fn extract_value(&mut self, agg_val: Self::Value, idx: u64) -> Self::Value {
        todo!()
    }
    fn insert_value(&mut self, agg_val: Self::Value, elt: Self::Value, idx: u64) -> Self::Value {
        todo!()
    }
    fn set_personality_fn(&mut self, personality: Self::Function) {
        todo!()
    }
    fn cleanup_landing_pad(&mut self, pers_fn: Self::Function) -> (Self::Value, Self::Value) {
        todo!()
    }
    fn filter_landing_pad(&mut self, pers_fn: Self::Function) {
        todo!()
    }
    fn resume(&mut self, exn0: Self::Value, exn1: Self::Value) {
        todo!()
    }
    fn cleanup_pad(&mut self, parent: Option<Self::Value>, args: &[Self::Value]) -> Self::Funclet {
        todo!()
    }
    fn cleanup_ret(&mut self, funclet: &Self::Funclet, unwind: Option<Self::BasicBlock>) {
        todo!()
    }
    fn catch_pad(&mut self, parent: Self::Value, args: &[Self::Value]) -> Self::Funclet {
        todo!()
    }
    fn catch_switch(
        &mut self,
        parent: Option<Self::Value>,
        unwind: Option<Self::BasicBlock>,
        handlers: &[Self::BasicBlock],
    ) -> Self::Value {
        todo!()
    }
    fn atomic_cmpxchg(
        &mut self,
        dst: Self::Value,
        cmp: Self::Value,
        src: Self::Value,
        order: AtomicOrdering,
        failure_order: AtomicOrdering,
        weak: bool,
    ) -> (Self::Value, Self::Value) {
        todo!()
    }
    fn atomic_rmw(
        &mut self,
        op: AtomicRmwBinOp,
        dst: Self::Value,
        src: Self::Value,
        order: AtomicOrdering,
        ret_ptr: bool,
    ) -> Self::Value {
        todo!()
    }
    fn atomic_fence(&mut self, order: AtomicOrdering, scope: SynchronizationScope) {
        todo!()
    }
    fn set_invariant_load(&mut self, load: Self::Value) {
        todo!()
    }
    fn lifetime_start(&mut self, ptr: Self::Value, size: Size) {
        todo!()
    }
    fn lifetime_end(&mut self, ptr: Self::Value, size: Size) {
        todo!()
    }
    fn call(
        &mut self,
        llty: Self::Type,
        fn_attrs: Option<&CodegenFnAttrs>,
        fn_abi: Option<&FnAbi<'tcx, Ty<'tcx>>>,
        fn_val: Self::Value,
        args: &[Self::Value],
        funclet: Option<&Self::Funclet>,
        instance: Option<Instance<'tcx>>,
    ) -> Self::Value {
        todo!()
    }
    fn tail_call(
        &mut self,
        llty: Self::Type,
        fn_attrs: Option<&CodegenFnAttrs>,
        fn_abi: &FnAbi<'tcx, Ty<'tcx>>,
        llfn: Self::Value,
        args: &[Self::Value],
        funclet: Option<&Self::Funclet>,
        instance: Option<Instance<'tcx>>,
    ) {
        todo!()
    }
    fn zext(&mut self, val: Self::Value, dest_ty: Self::Type) -> Self::Value {
        todo!()
    }
    fn apply_attrs_to_cleanup_callsite(&mut self, llret: Self::Value) {
        todo!()
    }
}
