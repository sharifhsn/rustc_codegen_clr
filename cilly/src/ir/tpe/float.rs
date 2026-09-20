use serde::{Deserialize, Serialize};

use crate::{IntoAsmIndex, bimap::Interned};

use super::super::{
    Assembly, CILNode, ClassRef, Const, MethodRef, Type,
    cilnode::MethodKind,
    hashable::{HashableF32, HashableF64},
};

#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Float {
    F16,
    F32,
    F64,
    F128,
}
impl Float {
    /// Returns a constant 0 of this float.
    #[must_use]
    pub fn zero(&self) -> Const {
        match self {
            Float::F16 => todo!(),
            Float::F32 => Const::F32(HashableF32(0.0)),
            Float::F64 => Const::F64(HashableF64(0.0)),
            Float::F128 => todo!(),
        }
    }
    /// Returns the size of this floating-point number, in bytes
    /// ```
    /// # use cilly::Float;
    /// assert_eq!(Float::F16.size(),2);
    /// assert_eq!(Float::F128.size(),16);
    /// ```
    #[must_use]
    pub fn size(&self) -> u8 {
        match self {
            Float::F16 => 2,
            Float::F32 => 4,
            Float::F64 => 8,
            Float::F128 => 16,
        }
    }
    /// Checks if this float is NaN
    pub fn is_nan(&self, val: Interned<CILNode>, asm: &mut Assembly) -> Interned<CILNode> {
        let method = self.method_ref(asm, "IsNaN", [Type::Float(*self)], Type::Bool);
        asm.alloc_node(CILNode::call(method, [val]))
    }
    /// Returns a short name of the float
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Float::F16 => "f16",
            Float::F32 => "f32",
            Float::F64 => "f64",
            Float::F128 => "f128",
        }
    }
    /// Clamps the float to a range <fmin,fmax>
    pub fn clamp(
        &self,
        val: Interned<CILNode>,
        fmin: Interned<CILNode>,
        fmax: Interned<CILNode>,
        asm: &mut Assembly,
    ) -> Interned<CILNode> {
        let method = self.method_ref(
            asm,
            "Clamp",
            [Type::Float(*self), Type::Float(*self), Type::Float(*self)],
            Type::Float(*self),
        );
        asm.alloc_node(CILNode::call(method, [val, fmin, fmax]))
    }
    /// Returns a class representing this flaoting-point type.
    pub fn class(&self, asm: &mut Assembly) -> Interned<ClassRef> {
        match self {
            Float::F16 => ClassRef::half(asm),
            Float::F32 => ClassRef::single(asm),
            Float::F64 => ClassRef::double(asm),
            Float::F128 => todo!(),
        }
    }
    /// Raises base to power.
    pub fn pow(
        &self,
        base: Interned<CILNode>,
        exp: Interned<CILNode>,
        asm: &mut Assembly,
    ) -> Interned<CILNode> {
        self.math2(base, exp, asm, "Pow")
    }
    /// Raises base to power.
    pub fn math2(
        &self,
        base: Interned<CILNode>,
        exp: Interned<CILNode>,
        asm: &mut Assembly,
        name: &str,
    ) -> Interned<CILNode> {
        let method = self.method_ref(
            asm,
            name,
            [Type::Float(*self), Type::Float(*self)],
            Type::Float(*self),
        );
        asm.alloc_node(CILNode::call(method, [base, exp]))
    }

    pub fn math1(
        &self,
        val: Interned<CILNode>,
        asm: &mut Assembly,
        name: &str,
    ) -> Interned<CILNode> {
        let method = self.method_ref(asm, name, [Type::Float(*self)], Type::Float(*self));
        asm.alloc_node(CILNode::call(method, [val]))
    }

    fn method_ref(
        &self,
        asm: &mut Assembly,
        name: &str,
        inputs: impl Into<Box<[Type]>>,
        output: Type,
    ) -> Interned<MethodRef> {
        let class = self.class(asm);
        let name = asm.alloc_string(name);
        let sig = asm.sig(inputs, output);
        asm.alloc_methodref(MethodRef::new(
            class,
            name,
            sig,
            MethodKind::Static,
            [].into(),
        ))
    }
    /// Counts the number of bits this number has.
    /// ```
    /// # use cilly::Float;
    /// assert_eq!(Float::F16.bits(),16);
    /// assert_eq!(Float::F128.bits(),128);
    /// ```
    pub fn bits(&self) -> u8 {
        self.size() * 8
    }
}
impl From<Float> for Type {
    fn from(value: Float) -> Self {
        Type::Float(value)
    }
}
impl IntoAsmIndex<Interned<Type>> for Float {
    fn into_idx(self, asm: &mut Assembly) -> Interned<Type> {
        asm.alloc_type(Type::Float(self))
    }
}
#[test]
fn name() {
    assert_eq!(Float::F16.name(), "f16");
    assert_eq!(Float::F32.name(), "f32");
    assert_eq!(Float::F64.name(), "f64");
    assert_eq!(Float::F128.name(), "f128");
}
