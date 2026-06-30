#[macro_export]
macro_rules! binop {
    // |closure| + |closure|
    (|$asm1:ident|$lhs:expr,|$asm2:ident|$rhs:expr,$op:expr) => {
        |asm: &mut $crate::asm::Assembly| {
            asm.biop(
                |$asm1: &mut $crate::asm::Assembly| $lhs.into_idx($asm1),
                |$asm2: &mut $crate::asm::Assembly| $rhs.into_idx($asm2),
                $op,
            )
        }
    };
    (|$asm1:ident|$lhs:block,|$asm2:ident|$rhs:expr,$op:expr) => {
        |asm: &mut $crate::asm::Assembly| {
            asm.biop(
                |$asm1: &mut $crate::asm::Assembly| $lhs.into_idx($asm1),
                |$asm2: &mut $crate::asm::Assembly| $rhs.into_idx($asm2),
                $op,
            )
        }
    };
    (|$asm1:ident|$lhs:expr,|$asm2:ident|$rhs:block,$op:expr) => {
        |asm: &mut $crate::asm::Assembly| {
            asm.biop(
                |$asm1: &mut $crate::asm::Assembly| $lhs.into_idx($asm1),
                |$asm2: &mut $crate::asm::Assembly| $rhs.into_idx($asm2),
                $op,
            )
        }
    };
    (|$asm1:ident|$lhs:block,|$asm2:ident|$rhs:block,$op:expr) => {
        |asm: &mut $crate::asm::Assembly| {
            asm.biop(
                |$asm1: &mut $crate::asm::Assembly| $lhs.into_idx($asm1),
                |$asm2: &mut $crate::asm::Assembly| $rhs.into_idx($asm2),
                $op,
            )
        }
    };
    // block + |closure|
    ($lhs:expr,|$asm2:ident|$rhs:expr,$op:expr) => {
        |asm: &mut $crate::asm::Assembly| {
            asm.biop(
                $lhs,
                |$asm2: &mut $crate::asm::Assembly| $rhs.into_idx($asm2),
                $op,
            )
        }
    };
    ($lhs:block,|$asm2:ident|$rhs:expr,$op:expr) => {
        |asm: &mut $crate::asm::Assembly| {
            asm.biop(
                $lhs,
                |$asm2: &mut $crate::asm::Assembly| $rhs.into_idx($asm2),
                $op,
            )
        }
    };
    ($lhs:expr,|$asm2:ident|$rhs:block,$op:expr) => {
        |asm: &mut $crate::asm::Assembly| {
            asm.biop(
                $lhs,
                |$asm2: &mut $crate::asm::Assembly| $rhs.into_idx($asm2),
                $op,
            )
        }
    };
    ($lhs:block,|$asm2:ident|$rhs:block,$op:expr) => {
        |asm: &mut $crate::asm::Assembly| {
            asm.biop(
                $lhs,
                |$asm2: &mut $crate::asm::Assembly| $rhs.into_idx($asm2),
                $op,
            )
        }
    };
    // block + |closure|
    (|$asm1:ident|$lhs:expr,$rhs:expr,$op:expr) => {
        |asm: &mut $crate::asm::Assembly| {
            asm.biop(
                |$asm1: &mut $crate::asm::Assembly| $lhs.into_idx($asm1),
                rhs,
                $op,
            )
        }
    };
    (|$asm1:ident|$lhs:expr,$rhs:block,$op:expr) => {
        |asm: &mut $crate::asm::Assembly| {
            asm.biop(
                |$asm1: &mut $crate::asm::Assembly| $lhs.into_idx($asm1),
                rhs,
                $op,
            )
        }
    };
    (|$asm1:ident|$lhs:block,$rhs:expr,$op:expr) => {
        |asm: &mut $crate::asm::Assembly| {
            asm.biop(
                |$asm1: &mut $crate::asm::Assembly| $lhs.into_idx($asm1),
                rhs,
                $op,
            )
        }
    };
    (|$asm1:ident|$lhs:block,$rhs:block,$op:expr) => {
        |asm: &mut $crate::asm::Assembly| {
            asm.biop(
                |$asm1: &mut $crate::asm::Assembly| $lhs.into_idx($asm1),
                rhs,
                $op,
            )
        }
    };
    // block + block
    ($lhs:expr,$rhs:expr,$op:expr) => {
        |asm: &mut $crate::asm::Assembly| asm.biop($lhs, $rhs, $op)
    };
    ($lhs:block,$rhs:expr,$op:expr) => {
        |asm: &mut $crate::asm::Assembly| asm.biop($lhs, $rhs, $op)
    };
    ($lhs:expr,$rhs:block,$op:expr) => {
        |asm: &mut $crate::asm::Assembly| asm.biop($lhs, $rhs, $op)
    };
    ($lhs:block,$rhs:block,$op:expr) => {
        |asm: &mut $crate::asm::Assembly| asm.biop($lhs, $rhs, $op)
    };
}
#[macro_export]
macro_rules! gen_binop {
    ($name:ident,$op:expr) => {
        #[macro_export]
        macro_rules! $name {
                        // |closure| + |closure|
                        (|$asm1:ident|$lhs:expr,|$asm2:ident|$rhs:expr) => {{
                            use $crate::BinOp;
                            $crate:binop!(|$asm1| $lhs, |$asm2| $rhs, $op)}
                        };
                        (|$asm1:ident|$lhs:block,|$asm2:ident|$rhs:expr) => {{
                            use $crate::BinOp;
                            $crate:binop!(|$asm1| $lhs, |$asm2| $rhs, $op)}
                        };
                        (|$asm1:ident|$lhs:expr,|$asm2:ident|$rhs:block) => {{  use $crate::BinOp;
                            $crate:binop!(|$asm1| $lhs, |$asm2| $rhs, $op)}
                        };
                        (|$asm1:ident|$lhs:block,|$asm2:ident|$rhs:block) => {{ use $crate::BinOp;
                            $crate:binop!(|$asm1| $lhs, |$asm2| $rhs, $op) }
                        };
                        // block + |closure|
                        ($lhs:expr,|$asm2:ident|$rhs:expr) => {{use $crate::BinOp;
                            $crate:binop!($lhs, |$asm2| $rhs, $op)  }
                        };
                        (|$lhs:block,|$asm2:ident|$rhs:expr) => {{ use $crate::BinOp;
                            $crate:binop!($lhs, |$asm2| $rhs, $op) }
                        };
                        ($lhs:expr,|$asm2:ident|$rhs:block) => {{ use $crate::BinOp;
                            $crate:binop!($lhs, |$asm2| $rhs, $op)}
                        };
                        ($lhs:block,|$asm2:ident|$rhs:block) => {{ use $crate::BinOp;
                            $crate:binop!($lhs, |$asm2| $rhs, $op)}
                        };
                        // |closure| + block
                        (|$asm1:ident|$lhs:expr,$rhs:expr) => {{ use $crate::BinOp;
                            $crate:binop!(|$asm1| $lhs, $rhs, $op)}
                        };
                        (|$asm1:ident|$lhs:block,$rhs:expr) => {{ use $crate::BinOp;
                            $crate:binop!(|$asm1| $lhs, $rhs, $op)}
                        };
                        (|$asm1:ident|$lhs:expr,|$asm2:ident|$rhs:block) => {{ use $crate::BinOp;
                            $crate:binop!(|$asm1| $lhs, $rhs, $op)}
                        };
                        (|$asm1:ident|$lhs:block,|$asm2:ident|$rhs:block) => {{ use $crate::BinOp;
                            $crate:binop!(|$asm1| $lhs, $rhs, $op)}
                        }; // block + block
                        ($lhs:expr,$rhs:expr) => {{ use $crate::BinOp;
                            $crate::binop!({$lhs}, $rhs, $op)}
                        };
                        ($lhs:block,$rhs:expr) => {{ use $crate::BinOp;
                            $crate:binop!( $lhs, $rhs, $op)}
                        };
                        ($lhs:expr,|$asm2:ident|$rhs:block) => {{ use $crate::BinOp;
                            $crate:binop!( $lhs, $rhs, $op)}
                        };
                        ($lhs:block,|$asm2:ident|$rhs:block) => {{ use $crate::BinOp;
                            $crate:binop!( $lhs, $rhs, $op)}
                        };
                        }
    };
}
#[macro_export]
macro_rules! size_of {
    (()) => {
        compile_error!("Attempt to take the size of void type (), which is not allowed")
    };
    (usize) => {{
        use $crate::IntoAsmIndex;
        |asm: &mut $crate::asm::Assembly| {
            <$crate::CILNode as IntoAsmIndex<$crate::ir::Interned<$crate::ir::CILNode>>>::into_idx(
                $crate::CILNode::SizeOf(asm.alloc_type($crate::Type::Int($crate::Int::USize))),
                asm,
            )
        }
    }};
    (|$asm:ident|$val:expr) => {
        |asm: &mut $crate::asm::Assembly| {
            <$crate::CILNode as IntoAsmIndex<$crate::Interned<CILNode>>>::into_idx(
                $crate::CILNode::SizeOf(|$asm| { $val }.into_idx(asm)),
            )
        }
    };
    (|$asm:ident|$val:block) => {
        |asm: &mut $crate::asm::Assembly| {
            <$crate::CILNode as IntoAsmIndex<$crate::Interned<CILNode>>>::into_idx(
                $crate::CILNode::SizeOf(|$asm| { $val }.into_idx(asm)),
            )
        }
    };
    ($val:expr) => {{
        use $crate::IntoAsmIndex;
        |asm: &mut $crate::asm::Assembly| {
            <$crate::CILNode as IntoAsmIndex<$crate::ir::Interned<$crate::ir::CILNode>>>::into_idx(
                $crate::CILNode::SizeOf($val.into_idx(asm)),
                asm,
            )
        }
    }};
    ($val:block) => {
        |asm: &mut $crate::asm::Assembly| {
            <$crate::CILNode as IntoAsmIndex<$crate::Interned<CILNode>>>::into_idx(
                $crate::CILNode::SizeOf($val.into_idx(asm)),
            )
        }
    };
}
gen_binop! {add,  BinOp::Add}
gen_binop! {sub,  BinOp::Sub}
gen_binop! {mul, BinOp::Mul}
#[macro_export]
macro_rules! zero_extend {
    (|$asm:ident|$val:expr,$ty:ty) => {{
        #[allow(unused_must_use)]
        {
            |asm: &mut $crate::asm::Assembly| {
                use $crate::IntoAsmIndex;

                    asm.int_cast(
                        |$asm| $val,
                        <$ty as $crate::IntoIntType>::int_type(),
                        $crate::cilnode::ExtendKind::ZeroExtend,
                    ),
                    asm,

            }
        }
    }};
    (|$asm:ident|$val:block,$ty:ty) => {{
        #[allow(unused_must_use)]
        {
            |asm: &mut $crate::asm::Assembly| {
                use $crate::IntoAsmIndex;
                <$crate::CILNode as IntoAsmIndex<$crate::Interned<CILNode>>>::into_idx(
                    asm.int_cast(
                        |$asm| $val,
                        <$ty as $crate::IntoIntType>::int_type(),
                        $crate::cilnode::ExtendKind::ZeroExtend,
                    ),
                    asm,
                )
            }
        }
    }};
    ($val:expr,$ty:ty) => {{
        #[allow(unused_must_use)]
        {
            |asm: &mut $crate::asm::Assembly| {


                    asm.int_cast(
                        $val,
                        <$ty as $crate::IntoIntType>::int_type(),
                        $crate::cilnode::ExtendKind::ZeroExtend,
                    )


            }
        }
    }};
    ($val:block,$ty:ty) => {{
        #[allow(unused_must_use)]
        {
            |asm: &mut $crate::asm::Assembly| {
                use $crate::IntoAsmIndex;
                <$crate::CILNode as IntoAsmIndex<$crate::Interned<CILNode>>>::into_idx(
                    asm.int_cast(
                        $val,
                        <$ty as $crate::IntoIntType>::int_type(),
                        $crate::cilnode::ExtendKind::ZeroExtend,
                    ),
                    asm,
                )
            }
        }
    }};
}
#[macro_export]
macro_rules! ptr_cast {
    ($val:expr,*$ptr:expr) => {
        |asm: &mut $crate::asm::Assembly| {
            use $crate::IntoAsmIndex;
            <$crate::CILNode as IntoAsmIndex<$crate::Interned<CILNode>>>::into_idx(
                asm.ptr_cast($val, $crate::cilnode::PtrCastRes::Ptr($ptr)),
                asm,
            )
        }
    };
}
#[macro_export]
macro_rules! ld_arg {
    ($literal:literal) => {
        CILNode::LdArg($literal)
    };
}
/// Generates the near-identical `ClassRef` constructors of shape
/// `pub fn NAME(asm: &mut Assembly) -> Interned<ClassRef>` whose body is just
/// `asm.alloc_class_ref(ClassRef::new(asm.alloc_string(TYPE_NAME),
/// Some(asm.alloc_string(ASM_NAME)), IS_VALUETYPE, GENERICS.into()))`.
///
/// Each row varies only by: the .NET type-name string, the assembly string
/// (defaults to `"System.Runtime"`, used by the majority of rows), the
/// valuetype flag (`value` => value type, `class` => reference type), and an
/// optional generic-parameter list. Load-bearing doc-comments are preserved
/// verbatim by writing them above the row — they are forwarded to the generated
/// associated function.
///
/// Row forms (each may be prefixed with `#[doc = "…"]` / `///` doc lines):
/// - `NAME => "TypeName", class;`                       (asm = System.Runtime)
/// - `NAME => "TypeName", value;`                       (asm = System.Runtime)
/// - `NAME => "TypeName", "AsmName", class;`
/// - `NAME => "TypeName", "AsmName", value;`
/// - `NAME => "TypeName", class, generics(element);`    (one generic, default asm)
/// - `NAME => "TypeName", value, generics(element);`
/// - `NAME => "TypeName", "AsmName", class, generics(element);`
/// - `NAME => "TypeName", "AsmName", value, generics(element);`
/// - `NAME => "TypeName", class, generics(key, value);` (two generics, default asm)
/// - `NAME => "TypeName", value, generics(key, value);`
/// - `NAME => "TypeName", "AsmName", class, generics(key, value);`
/// - `NAME => "TypeName", "AsmName", value, generics(key, value);`
#[macro_export]
macro_rules! bcl_class {
    // ---- internal valuetype-token -> bool normalization ----
    (@vt class) => { false };
    (@vt value) => { true };

    // ---- single-row emitters ----

    // no generics, explicit asm
    (@one
        $(#[$meta:meta])*
        $name:ident => $tname:literal, $asm:literal, $vt:tt
    ) => {
        $(#[$meta])*
        pub fn $name(asm: &mut $crate::ir::Assembly) -> $crate::ir::Interned<$crate::ir::ClassRef> {
            let name = asm.alloc_string($tname);
            let asm_name = Some(asm.alloc_string($asm));
            asm.alloc_class_ref($crate::ir::ClassRef::new(
                name,
                asm_name,
                $crate::bcl_class!(@vt $vt),
                [].into(),
            ))
        }
    };
    // no generics, default asm (System.Runtime)
    (@one
        $(#[$meta:meta])*
        $name:ident => $tname:literal, $vt:tt
    ) => {
        $crate::bcl_class!(@one $(#[$meta])* $name => $tname, "System.Runtime", $vt);
    };

    // one generic, explicit asm
    (@one
        $(#[$meta:meta])*
        $name:ident => $tname:literal, $asm:literal, $vt:tt, generics($element:ident)
    ) => {
        $(#[$meta])*
        pub fn $name(asm: &mut $crate::ir::Assembly, $element: $crate::ir::Type)
            -> $crate::ir::Interned<$crate::ir::ClassRef>
        {
            let name = asm.alloc_string($tname);
            let asm_name = Some(asm.alloc_string($asm));
            asm.alloc_class_ref($crate::ir::ClassRef::new(
                name,
                asm_name,
                $crate::bcl_class!(@vt $vt),
                [$element].into(),
            ))
        }
    };
    // one generic, default asm
    (@one
        $(#[$meta:meta])*
        $name:ident => $tname:literal, $vt:tt, generics($element:ident)
    ) => {
        $crate::bcl_class!(@one $(#[$meta])* $name => $tname, "System.Runtime", $vt, generics($element));
    };

    // two generics, explicit asm
    (@one
        $(#[$meta:meta])*
        $name:ident => $tname:literal, $asm:literal, $vt:tt, generics($k:ident, $v:ident)
    ) => {
        $(#[$meta])*
        pub fn $name(asm: &mut $crate::ir::Assembly, $k: $crate::ir::Type, $v: $crate::ir::Type)
            -> $crate::ir::Interned<$crate::ir::ClassRef>
        {
            let name = asm.alloc_string($tname);
            let asm_name = Some(asm.alloc_string($asm));
            asm.alloc_class_ref($crate::ir::ClassRef::new(
                name,
                asm_name,
                $crate::bcl_class!(@vt $vt),
                [$k, $v].into(),
            ))
        }
    };
    // two generics, default asm
    (@one
        $(#[$meta:meta])*
        $name:ident => $tname:literal, $vt:tt, generics($k:ident, $v:ident)
    ) => {
        $crate::bcl_class!(@one $(#[$meta])* $name => $tname, "System.Runtime", $vt, generics($k, $v));
    };

    // ---- table body tt-muncher: peel off one complete `;`-terminated row, then
    //      recurse on the remainder. Matches each concrete row shape directly so
    //      recursion depth is O(#rows), not O(#tokens). ----
    // done
    (@rows) => {};
    // two generics, explicit asm
    (@rows
        $(#[$meta:meta])*
        $name:ident => $tname:literal, $asm:literal, $vt:tt, generics($k:ident, $v:ident) ;
        $($rest:tt)*
    ) => {
        $crate::bcl_class!(@one $(#[$meta])* $name => $tname, $asm, $vt, generics($k, $v));
        $crate::bcl_class!(@rows $($rest)*);
    };
    // two generics, default asm
    (@rows
        $(#[$meta:meta])*
        $name:ident => $tname:literal, $vt:tt, generics($k:ident, $v:ident) ;
        $($rest:tt)*
    ) => {
        $crate::bcl_class!(@one $(#[$meta])* $name => $tname, $vt, generics($k, $v));
        $crate::bcl_class!(@rows $($rest)*);
    };
    // one generic, explicit asm
    (@rows
        $(#[$meta:meta])*
        $name:ident => $tname:literal, $asm:literal, $vt:tt, generics($element:ident) ;
        $($rest:tt)*
    ) => {
        $crate::bcl_class!(@one $(#[$meta])* $name => $tname, $asm, $vt, generics($element));
        $crate::bcl_class!(@rows $($rest)*);
    };
    // one generic, default asm
    (@rows
        $(#[$meta:meta])*
        $name:ident => $tname:literal, $vt:tt, generics($element:ident) ;
        $($rest:tt)*
    ) => {
        $crate::bcl_class!(@one $(#[$meta])* $name => $tname, $vt, generics($element));
        $crate::bcl_class!(@rows $($rest)*);
    };
    // no generics, explicit asm
    (@rows
        $(#[$meta:meta])*
        $name:ident => $tname:literal, $asm:literal, $vt:tt ;
        $($rest:tt)*
    ) => {
        $crate::bcl_class!(@one $(#[$meta])* $name => $tname, $asm, $vt);
        $crate::bcl_class!(@rows $($rest)*);
    };
    // no generics, default asm
    (@rows
        $(#[$meta:meta])*
        $name:ident => $tname:literal, $vt:tt ;
        $($rest:tt)*
    ) => {
        $crate::bcl_class!(@one $(#[$meta])* $name => $tname, $vt);
        $crate::bcl_class!(@rows $($rest)*);
    };

    // ---- table entry point: emit one impl block holding every row ----
    (
        impl $self:ident {
            $($body:tt)*
        }
    ) => {
        impl $self {
            $crate::bcl_class!(@rows $($body)*);
        }
    };
}
#[test]
fn macro_test() {
    let sum = add!(
        zero_extend!(size_of!(usize), usize),
        zero_extend!(size_of!(crate::Type::Int(crate::Int::U8)), usize)
    );
    let mut asm = super::Assembly::default();
    sum(&mut asm);
}
/*


gen_binop! {div,  crate::BinOp::Div}
gen_binop! {div_un,  crate::BinOp::Div}
gen_binop! {rem,  crate::BinOp::Rem}
gen_binop! {rem_un,  crate::BinOp::RemUn}
 */
