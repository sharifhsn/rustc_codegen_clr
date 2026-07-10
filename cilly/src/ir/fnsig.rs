use serde::{Deserialize, Serialize};

use super::{
    asm_link::{RelocateCtx, RelocateValue},
    Assembly, Type,
};
#[derive(Hash, PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct FnSig {
    inputs: Box<[Type]>,
    output: Type,
}

impl RelocateValue for FnSig {
    type Output = Self;

    fn relocate(self, ctx: &mut RelocateCtx<'_>, destination: &mut Assembly) -> Self {
        let Self { inputs, output } = self;
        Self {
            inputs: inputs
                .iter()
                .map(|tpe| destination.translate_type(ctx, *tpe))
                .collect(),
            output: destination.translate_type(ctx, output),
        }
    }
}

impl FnSig {
    #[must_use]
    pub fn new(input: impl Into<Box<[Type]>>, output: Type) -> Self {
        Self {
            inputs: input.into(),
            output,
        }
    }

    #[must_use]
    pub fn inputs(&self) -> &[Type] {
        &self.inputs
    }

    #[must_use]
    pub fn output(&self) -> &Type {
        &self.output
    }
    /// Itereates trough all the inputs of this sig.
    /// ```
    /// # use cilly::{Type,FnSig};
    /// let sig = FnSig::new([Type::PlatformString],Type::Void);
    /// assert_eq!(sig.iter_types().next(),Some(Type::PlatformString));
    /// ```
    pub fn iter_types(&self) -> impl Iterator<Item = Type> + '_ {
        self.inputs()
            .iter()
            .copied()
            .chain(std::iter::once(*self.output()))
    }

    pub fn inputs_mut(&mut self) -> &mut Box<[Type]> {
        &mut self.inputs
    }
    /// Changes the inputs of this function to *inputs*.
    /// ```
    /// # use cilly::{Type,FnSig};
    /// # let mut sig = FnSig::new([Type::PlatformString],Type::Void);
    /// assert_eq!(sig.inputs().len(),1);
    /// sig.set_inputs([Type::PlatformString,Type::PlatformChar]);
    /// assert_eq!(sig.inputs().len(),2);
    /// ```
    pub fn set_inputs(&mut self, inputs: impl Into<Box<[Type]>>) {
        self.inputs = inputs.into();
    }
}
