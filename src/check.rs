//! Typechecking for our DDL
//!
//! # Syntax
//!
//! ## Kinds
//!
//! ```plain
//! κ ::=
//!         Type        kind of types
//! ```
//!
//! ## Expressions
//!
//! ```plain
//! e ::=
//!         x                   variables
//!         ℕ                   natural number
//!         true                true value
//!         false               false value
//!         -e                  negation
//!         ¬e                  not
//!         op(Rel, e₁, e₂)     relational binary operation
//!         op(Cmp, e₁, e₂)     comparison binary operation
//!         op(Arith, e₁, e₂)   arithmetic binary operation
//!
//! Rel ::=
//!         ∨                   disjunction operator
//!         ∧                   conjunction operator
//!
//! Cmp ::=
//!         =                   equality operator
//!         ≠                   inequality operator
//!         <                   less than operator
//!         ≤                   less than or equal operator
//!         >                   greater than operator
//!         ≥                   greater than or equal operator
//!
//! Arith ::=
//!         +                   addition operator
//!         -                   subtraction operator
//!         *                   multiplication operator
//!         /                   division operator
//! ```
//!
//! ## Types
//!
//! ```plain
//! E ::=
//!         Le                  little endian
//!         Be                  big endian
//!
//! c ::=
//!         Bool                booleans
//!         UInt(ℕ, E)          unsigned integer with byte size and endianness
//!         Int(ℕ, E)           signed integer with byte size and endianness
//!         SingletonUInt(n)    a single unsigned integer
//!
//! τ ::=
//!         c                   type constants
//!         α                   variables
//!         τ₁ + τ₂             sum
//!         Σ x:τ₁ .τ₂          dependent pair
//!         [τ; e]              array
//!         { x:τ | e }         constrained type
//! ```
//!
//! In the `ast`, we represent the above as the following:
//!
//! - `Type::Var`: variables
//!
//! - `Type::Union`: series of unions
//!
//! - `Type::Struct`: nested dependent pairs
//!
//!   For example, the struct:
//!
//!   ```plain
//!   struct { len : u16, reserved : u16, data : [u16; len] }
//!   ```
//!
//!   Would be desugared into:
//!
//!   ```plain
//!   Σ len:u16 . Σ reserved:u16 . [u16; len]
//!   ```
//!
//!   Note how later fields have access to the data in previous fields.
//!
//! - `Type::Array`: TODO
//!
//! - `Type::Where`: constrained type

use ast::{Binop, Const, Definition, Expr, Kind, Type, Unop};
use env::Env;
use source::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KindError {
    Type(TypeError),
    UnboundType(Span, String),
    ArraySizeExpectedUInt(Span, Type),
    WherePredicateExpectedBool(Span, Type),
}

impl From<TypeError> for KindError {
    fn from(src: TypeError) -> KindError {
        KindError::Type(src)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeError {
    UnboundVariable(Span, String),
    UnexpectedUnaryOperand(Span, Unop, Type),
    UnexpectedBinaryLhs(Span, Type),
    UnexpectedBinaryRhs(Span, Type),
}

/// The subtyping relation: `τ₁ <: τ₂`
///
/// # Rules
///
/// ```plain
/// ―――――――――――――――――――― (S-REFL)
///        τ <: τ
///
///
///                  ℕ₁ ≤ ℕ₂
/// ―――――――――――――――――――――――――――――――――――――――――― (S-SINGLETON-UINT)
///   SingletonUInt(ℕ₁) <: SingletonUInt(ℕ₂)
///
///
///         FIXME - check byte size
/// ――――――――――――――――――――――――――――――――――――― (S-UINT)
///    SingletonUInt(ℕ₂) <: UInt(ℕ₁, E)
///
///
///         FIXME - check byte size
/// ――――――――――――――――――――――――――――――――――――― (S-INT)
///    SingletonUInt(ℕ₂) <: Int(ℕ₁, E)
/// ```
pub fn is_subtype(sty: &Type, ty: &Type) -> bool {
    match (sty, ty) {
        // S-REFL
        (sty, ty) if sty == ty => true,

        // S-SINGLETON-UINT
        (&Type::SingletonUInt(sn), &Type::SingletonUInt(n)) if sn <= n => true,

        // S-UINT, S-INT
        (&Type::SingletonUInt(_), &Type::UInt(_, _)) |
        (&Type::SingletonUInt(_), &Type::SInt(_, _)) => true,

        (_, _) => false,
    }
}

impl<'parent> Env<'parent> {
    pub fn check_defs<I>(&mut self, defs: I) -> Result<(), KindError>
    where
        I: IntoIterator<Item = Definition>,
    {
        for def in defs {
            kind_of(self, &def.ty)?;
            self.add_ty(def.name, def.ty);
        }
        Ok(())
    }
}

/// The kinding relation: `Γ ⊢ τ : κ`
///
/// # Rules
///
/// ```plain
/// ―――――――――――――――――――― (K-CONST)
///     Γ ⊢ c : Type
///
///
///         α ∈ Γ
/// ―――――――――――――――――――― (K-VAR)
///     Γ ⊢ α : Type
///
///
///     Γ ⊢ τ₁ : Type        Γ ⊢ τ₂ : Type
/// ―――――――――――――――――――――――――――――――――――――――――― (K-SUM)
///              Γ ⊢ τ₁ + τ₂ : Type
///
///
///     Γ ⊢ τ₁ : Type        Γ, x:τ₁ ⊢ τ₂ : Type
/// ―――――――――――――――――――――――――――――――――――――――――――――――――― (K-DEPENDENT-PAIR)
///              Γ ⊢ Σ x:τ₁ .τ₂ : Type
///
///
///     Γ ⊢ τ : Type        Γ ⊢ e : UInt(ℕ, E)
/// ――――――――――――――――――――――――――――――――――――――――――――――― (K-ARRAY-UINT)
///               Γ ⊢ [τ; e] :
///
///
///     Γ ⊢ τ : Type       Γ ⊢ e : SingletonUInt(ℕ)
/// ―――――――――――――――――――――――――――――――――――――――――――――――――― (K-ARRAY-SINGLETON-UINT)
///               Γ ⊢ [τ; e] : Type
///
///
///     Γ ⊢ τ : Type      Γ, x:τ ⊢ b : Bool
/// ―――――――――――――――――――――――――――――――――――――――――― (K-CON)
///           Γ ⊢ { x:τ | b } : Type
/// ```
pub fn kind_of(env: &Env, ty: &Type) -> Result<Kind, KindError> {
    match *ty {
        // K-CONST
        Type::Bool => Ok(Kind::Type),
        Type::SingletonUInt(_) => Ok(Kind::Type),
        Type::UInt(_, _) => Ok(Kind::Type),
        Type::SInt(_, _) => Ok(Kind::Type),
        Type::Float(_, _) => Ok(Kind::Type),

        // K-VAR
        Type::Var(span, ref name) => {
            // TODO: kind of var?
            // α ∈ Γ
            match env.lookup_ty(name) {
                Some(_) => Ok(Kind::Type),
                None => Err(KindError::UnboundType(span, name.clone())),
            }
        }

        // K-SUM
        Type::Union(_, ref tys) => {
            for ty in tys {
                // Γ ⊢ τ₁ : Type
                kind_of(env, &ty)?;
            }
            Ok(Kind::Type)
        }

        // K-DEPENDENT-PAIR
        Type::Struct(_, ref fields) => {
            // TODO: prevent name shadowing?
            let mut inner_env = env.extend();
            for field in fields {
                // Γ ⊢ τ₁ : Type
                kind_of(&inner_env, &field.ty)?;
                // Γ, x:τ₁ ⊢ τ₂ : Type
                inner_env.add_binding(field.name.clone(), field.ty.clone());
            }
            Ok(Kind::Type)
        }

        // K-ARRAY-...
        Type::Array(span, ref ty, ref size) => {
            kind_of(env, ty)?;
            let expr_ty = type_of(env, size)?;

            match expr_ty {
                // K-ARRAY-SINGLETON-UINT
                Type::SingletonUInt(_) |
                // K-ARRAY-UINT
                Type::UInt(_, _) => Ok(Kind::Type),
                ty => Err(KindError::ArraySizeExpectedUInt(span, ty)),
            }
        }

        // K-CON
        Type::Where(span, ref ty, ref param, ref pred) => {
            kind_of(env, ty)?;

            let mut inner_env = env.extend();
            // TODO: prevent name shadowing?
            inner_env.add_binding(param.clone(), (**ty).clone());
            match type_of(env, pred)? {
                Type::Bool => Ok(Kind::Type),
                pred_ty => Err(KindError::WherePredicateExpectedBool(span, pred_ty)),
            }
        }
    }
}

/// The typing relation: `Γ ⊢ e : τ`
///
/// # Rules
///
/// ```plain
/// ―――――――――――――――――――――――――――― (T-TRUE)
///       Γ ⊢ true : Bool
///
///
/// ―――――――――――――――――――――――――――― (T-FALSE)
///       Γ ⊢ false : Bool
///
///
/// ―――――――――――――――――――――――――――― (T-SINGLETON-UINT)
///   Γ ⊢ ℕ : SingletonUInt(ℕ)
///
///
///           x : τ ∈ Γ
/// ―――――――――――――――――――――――――――― (T-VAR)
///           Γ ⊢ x : τ
///
///
///         Γ ⊢ e : Bool
/// ―――――――――――――――――――――――――――― (T-NOT)
///         Γ ⊢ ¬e : Bool
///
///
///     Γ ⊢ e : τ       SingletonUInt(ℕ) <: τ
/// ――――――――――――――――――――――――――――――――――――――――― (T-NEG)
///              Γ ⊢ -e : τ
///
///
///      Γ ⊢ e₁ : Bool       Γ ⊢ e₂ : Bool
/// ――――――――――――――――――――――――――――――――――――――――― (T-REL)
///         Γ ⊢ op(Rel, e₁, e₂) : Bool
///
///
///   Γ ⊢ e₁ : τ₁     Γ ⊢ e₂ : τ₂      τ₁ <: τ₂      SingletonUInt(ℕ) <: τ₂
/// ―――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――― (T-CMP-LHS)
///                      Γ ⊢ op(Cmp, e₁, e₂) : Bool
///
///
///   Γ ⊢ e₁ : τ₁     Γ ⊢ e₂ : τ₂      τ₂ <: τ₁      SingletonUInt(ℕ) <: τ₁
/// ―――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――― (T-CMP-RHS)
///                      Γ ⊢ op(Cmp, e₁, e₂) : Bool
///
///
///   Γ ⊢ e₁ : τ₁    Γ ⊢ e₂ : τ₂      τ₁ <: τ₂      SingletonUInt(ℕ) <: τ₂
/// ――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――― (T-ARITH-LHS)
///                    Γ ⊢ op(Arith, e₁, e₂) : τ₂
///
///
///   Γ ⊢ e₁ : τ₁    Γ ⊢ e₂ : τ₂      τ₂ <: τ₁      SingletonUInt(ℕ) <: τ₁
/// ――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――――― (T-ARITH-RHS)
///                    Γ ⊢ op(Arith, e₁, e₂) : τ₁
/// ```
pub fn type_of(env: &Env, expr: &Expr) -> Result<Type, TypeError> {
    match *expr {
        // T-TRUE, T-FALSE
        Expr::Const(_, Const::Bool(_)) => Ok(Type::Bool),

        // T-SINGLETON-UINT
        Expr::Const(_, Const::UInt(value)) => Ok(Type::SingletonUInt(value)),

        // T-VAR
        Expr::Var(span, ref name) => {
            match env.lookup_binding(name) {
                Some(ty) => Ok(ty.clone()),
                None => Err(TypeError::UnboundVariable(span, name.clone())),
            }
        }

        Expr::Unop(span, op, ref value) => {
            let value_ty = type_of(env, value)?;

            match op {
                // T-NOT
                Unop::Not => {
                    if value_ty == Type::Bool {
                        Ok(Type::Bool)
                    } else {
                        Err(TypeError::UnexpectedUnaryOperand(span, op, value_ty))
                    }
                }
                // T-NEG
                Unop::Neg => {
                    if is_subtype(&Type::SingletonUInt(0), &value_ty) {
                        Ok(value_ty)
                    } else {
                        Err(TypeError::UnexpectedUnaryOperand(span, op, value_ty))
                    }
                }
            }
        }

        Expr::Binop(span, op, ref lhs, ref rhs) => {
            let lhs_ty = type_of(env, lhs)?;
            let rhs_ty = type_of(env, rhs)?;

            match op {
                // T-REL
                Binop::Or | Binop::And => {
                    if lhs_ty != Type::Bool {
                        Err(TypeError::UnexpectedBinaryLhs(span, lhs_ty))
                    } else if rhs_ty != Type::Bool {
                        Err(TypeError::UnexpectedBinaryRhs(span, rhs_ty))
                    } else {
                        Ok(Type::Bool)
                    }
                }
                // T-CMP-...
                Binop::Eq | Binop::Ne | Binop::Le | Binop::Lt | Binop::Gt | Binop::Ge => {
                    let unknown_int = Type::SingletonUInt(0);

                    // T-CMP-LHS
                    if is_subtype(&lhs_ty, &rhs_ty) && is_subtype(&unknown_int, &rhs_ty) {
                        Ok(Type::Bool)
                    // T-CMP-RHS
                    } else if is_subtype(&rhs_ty, &lhs_ty) && is_subtype(&unknown_int, &lhs_ty) {
                        Ok(Type::Bool)
                    } else {
                        unimplemented!() // FIXME: Better errors
                    }
                }
                // T-ARITH-...
                Binop::Add | Binop::Sub | Binop::Mul | Binop::Div => {
                    let unknown_int = Type::SingletonUInt(0);

                    // T-ARITH-LHS
                    if is_subtype(&lhs_ty, &rhs_ty) && is_subtype(&unknown_int, &rhs_ty) {
                        Ok(rhs_ty)
                    // T-ARITH-RHS
                    } else if is_subtype(&rhs_ty, &lhs_ty) && is_subtype(&unknown_int, &lhs_ty) {
                        Ok(lhs_ty)
                    } else {
                        unimplemented!() // FIXME: Better errors
                    }
                }
            }
        }
    }
}

#[cfg(test)]
pub mod tests {
    use ast::Endianness;
    use parser;
    use source::BytePos as B;
    use super::*;

    // Add expressions

    mod type_of {
        use super::*;

        mod add_expr {
            use super::*;

            #[test]
            fn uint_with_uint() {
                let mut env = Env::default();
                let len_ty = Type::UInt(32, Endianness::Target);
                env.add_binding("len", len_ty.clone());

                let expr = parser::parse_expr(&env, "len + len").unwrap();
                assert_eq!(type_of(&env, &expr), Ok(len_ty));
            }

            #[test]
            fn unknown_with_uint() {
                let mut env = Env::default();
                let len_ty = Type::UInt(32, Endianness::Target);
                env.add_binding("len", len_ty.clone());

                let expr = parser::parse_expr(&env, "1 + len").unwrap();
                assert_eq!(type_of(&env, &expr), Ok(len_ty.clone()));
            }

            #[test]
            fn uint_with_unknown() {
                let mut env = Env::default();
                let len_ty = Type::UInt(32, Endianness::Target);
                env.add_binding("len", len_ty.clone());

                let expr = parser::parse_expr(&env, "len + 1").unwrap();
                assert_eq!(type_of(&env, &expr), Ok(len_ty.clone()));
            }

            #[test]
            fn unknown_with_unknown() {
                let env = Env::default();
                let expr = parser::parse_expr(&env, "1 + 1").unwrap();

                assert_eq!(type_of(&env, &expr), Ok(Type::SingletonUInt(1)));
            }
        }

        mod mul_expr {
            use super::*;

            #[test]
            fn uint_with_uint() {
                let mut env = Env::default();
                let len_ty = Type::UInt(32, Endianness::Target);
                env.add_binding("len", len_ty.clone());

                let expr = parser::parse_expr(&env, "len * len").unwrap();
                assert_eq!(type_of(&env, &expr), Ok(len_ty));
            }

            #[test]
            fn unknown_with_uint() {
                let mut env = Env::default();
                let len_ty = Type::UInt(32, Endianness::Target);
                env.add_binding("len", len_ty.clone());

                let expr = parser::parse_expr(&env, "1 * len").unwrap();
                assert_eq!(type_of(&env, &expr), Ok(len_ty.clone()));
            }

            #[test]
            fn uint_with_unknown() {
                let mut env = Env::default();
                let len_ty = Type::UInt(32, Endianness::Target);
                env.add_binding("len", len_ty.clone());

                let expr = parser::parse_expr(&env, "len * 1").unwrap();
                assert_eq!(type_of(&env, &expr), Ok(len_ty.clone()));
            }

            #[test]
            fn unknown_with_unknown() {
                let env = Env::default();
                let expr = parser::parse_expr(&env, "1 * 1").unwrap();

                assert_eq!(type_of(&env, &expr), Ok(Type::SingletonUInt(1)));
            }
        }
    }

    mod kind_of {
        use super::*;

        #[test]
        fn ty_const() {
            let env = Env::default();
            let ty = Type::SInt(16, Endianness::Target);

            assert_eq!(kind_of(&env, &ty), Ok(Kind::Type));
        }

        #[test]
        fn var() {
            let env = Env::default();
            let ty = parser::parse_ty(&env, "u8").unwrap();

            assert_eq!(kind_of(&env, &ty), Ok(Kind::Type));
        }

        #[test]
        fn var_missing() {
            let env = Env::default();
            let ty = parser::parse_ty(&env, "Foo").unwrap();

            assert_eq!(
                kind_of(&env, &ty),
                Err(KindError::UnboundType(
                    Span::new(B(0), B(3)),
                    "Foo".to_owned(),
                ))
            );
        }

        #[test]
        fn union() {
            let env = Env::default();
            let ty = parser::parse_ty(&env, "union { u8, u16, i32 }").unwrap();

            assert_eq!(kind_of(&env, &ty), Ok(Kind::Type));
        }

        #[test]
        fn union_element_missing() {
            let env = Env::default();
            let ty = parser::parse_ty(&env, "union { u8, Foo, i32 }").unwrap();

            assert_eq!(
                kind_of(&env, &ty),
                Err(KindError::UnboundType(
                    Span::new(B(12), B(15)),
                    "Foo".to_owned(),
                ))
            );
        }

        #[test]
        fn pair() {
            let env = Env::default();
            let ty = parser::parse_ty(&env, "struct { x: u8, y: u8 }").unwrap();

            assert_eq!(kind_of(&env, &ty), Ok(Kind::Type));
        }

        #[test]
        fn dependent_pair() {
            let env = Env::default();
            let ty = parser::parse_ty(&env, "struct { len: u8, data: [u8; len] }").unwrap();

            assert_eq!(kind_of(&env, &ty), Ok(Kind::Type));
        }

        #[test]
        fn array() {
            let env = Env::default();
            let ty = parser::parse_ty(&env, "[u8; 16]").unwrap();

            assert_eq!(kind_of(&env, &ty), Ok(Kind::Type));
        }

        #[test]
        fn array_len() {
            let mut env = Env::default();
            let len_ty = Type::UInt(32, Endianness::Target);
            env.add_binding("len", len_ty);
            let ty = parser::parse_ty(&env, "[u8; len]").unwrap();

            assert_eq!(kind_of(&env, &ty), Ok(Kind::Type));
        }

        #[test]
        fn array_singned_int_size() {
            let mut env = Env::default();
            let len_ty = parser::parse_ty(&env, "i8").unwrap();
            env.add_binding("len", len_ty.clone());
            let ty = parser::parse_ty(&env, "[u8; len]").unwrap();

            assert_eq!(
                kind_of(&env, &ty),
                Err(KindError::ArraySizeExpectedUInt(
                    Span::new(B(0), B(9)),
                    len_ty,
                ))
            );
        }

        #[test]
        fn array_struct_size() {
            let mut env = Env::default();
            let len_ty = parser::parse_ty(&env, "struct {}").unwrap();
            env.add_binding("len", len_ty.clone());
            let ty = parser::parse_ty(&env, "[u8; len]").unwrap();

            assert_eq!(
                kind_of(&env, &ty),
                Err(KindError::ArraySizeExpectedUInt(
                    Span::new(B(0), B(9)),
                    len_ty,
                ))
            );
        }
    }
}
