//! Type and kind-checking for our DDL

use syntax::{binary, host};
use syntax::{Binding, Ctx, Definition, Name, Named, Var};

#[cfg(test)]
mod tests;

// Typing

/// An error that was encountered during type checking
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeError<N> {
    /// A variable of the requested name was not bound in this scope
    UnboundVariable { expr: host::Expr<N>, name: N },
    /// Variable bound in the context was not at the value level
    ExprBindingExpected {
        expr: host::Expr<N>,
        found: Named<N, Binding<N>>,
    },
    /// One type was expected, but another was found
    Mismatch {
        expr: host::Expr<N>,
        found: host::Type<N>,
        expected: host::Type<N>,
    },
    /// Unexpected operand types in a binary operator expression
    BinopOperands {
        expr: host::Expr<N>,
        lhs_ty: host::Type<N>,
        rhs_ty: host::Type<N>,
    },
    /// A field was missing when projecting on a record
    MissingField {
        struct_expr: host::Expr<N>,
        struct_ty: host::Type<N>,
        field_name: N,
    },
}

/// Returns the type of a host expression, checking that it is properly formed
/// in the environment
pub fn ty_of<N: Name>(ctx: &Ctx<N>, expr: &host::Expr<N>) -> Result<host::Type<N>, TypeError<N>> {
    use syntax::host::{Binop, Const, Expr, Type, TypeConst, Unop};

    match *expr {
        // Constants are easy!
        Expr::Const(Const::Bit(_)) => Ok(Type::bit()),
        Expr::Const(Const::Bool(_)) => Ok(Type::bool()),
        Expr::Const(Const::Int(_)) => Ok(Type::int()),

        // Variables
        Expr::Var(Var::Free(ref x)) => Err(TypeError::UnboundVariable {
            expr: expr.clone(),
            name: x.clone(),
        }),
        Expr::Var(Var::Bound(Named(_, i))) => match ctx.lookup_ty(i) {
            Ok(Named(_, ty)) => Ok(ty.clone()),
            Err(Named(name, binding)) => Err(TypeError::ExprBindingExpected {
                expr: expr.clone(),
                found: Named(name.clone(), binding.clone()),
            }),
        },

        // Primitive expressions
        Expr::Prim(_, ref repr_ty) => Ok((**repr_ty).clone()),

        // Unary operators
        Expr::Unop(op, ref expr) => match op {
            Unop::Neg => match ty_of(ctx, &**expr)? {
                Type::Const(TypeConst::Int) => Ok(Type::int()),
                expr_ty => Err(TypeError::Mismatch {
                    expr: (**expr).clone(),
                    found: expr_ty,
                    expected: Type::int(),
                }),
            },
            Unop::Not => match ty_of(ctx, &**expr)? {
                Type::Const(TypeConst::Bool) => Ok(Type::bool()),
                expr_ty => Err(TypeError::Mismatch {
                    expr: (**expr).clone(),
                    found: expr_ty,
                    expected: Type::bool(),
                }),
            },
        },

        // Binary operators
        Expr::Binop(op, ref lhs_expr, ref rhs_expr) => {
            let lhs_ty = ty_of(ctx, &**lhs_expr)?;
            let rhs_ty = ty_of(ctx, &**rhs_expr)?;

            match op {
                // Relational operators
                Binop::Or | Binop::And => match (lhs_ty, rhs_ty) {
                    (Type::Const(TypeConst::Bool), Type::Const(TypeConst::Bool)) => {
                        Ok(Type::bool())
                    }
                    (lhs_ty, rhs_ty) => Err(TypeError::BinopOperands {
                        expr: expr.clone(),
                        lhs_ty,
                        rhs_ty,
                    }),
                },

                // Comparison operators
                Binop::Eq | Binop::Ne | Binop::Le | Binop::Lt | Binop::Gt | Binop::Ge => match (
                    lhs_ty,
                    rhs_ty,
                ) {
                    (Type::Const(TypeConst::Bit), Type::Const(TypeConst::Bit)) |
                    (Type::Const(TypeConst::Bool), Type::Const(TypeConst::Bool)) |
                    (Type::Const(TypeConst::Int), Type::Const(TypeConst::Int)) => Ok(Type::bool()),
                    (lhs_ty, rhs_ty) => Err(TypeError::BinopOperands {
                        expr: expr.clone(),
                        lhs_ty,
                        rhs_ty,
                    }),
                },

                // Arithmetic operators
                Binop::Add | Binop::Sub | Binop::Mul | Binop::Div => match (lhs_ty, rhs_ty) {
                    (Type::Const(TypeConst::Int), Type::Const(TypeConst::Int)) => Ok(Type::int()),
                    (lhs_ty, rhs_ty) => Err(TypeError::BinopOperands {
                        expr: expr.clone(),
                        lhs_ty,
                        rhs_ty,
                    }),
                },
            }
        }

        // Field projection
        Expr::Proj(ref struct_expr, ref field_name) => {
            let struct_ty = ty_of(ctx, &**struct_expr)?;

            match struct_ty.lookup_field(field_name).cloned() {
                Some(field_ty) => Ok(field_ty),
                None => Err(TypeError::MissingField {
                    struct_expr: (**struct_expr).clone(),
                    struct_ty: struct_ty.clone(),
                    field_name: field_name.clone(),
                }),
            }
        }

        // Abstraction
        Expr::Abs(Named(ref param_name, ref param_ty), ref body_expr) => {
            // FIXME: avoid cloning the environment
            let mut ctx = ctx.clone();
            ctx.extend(param_name.clone(), Binding::Expr((**param_ty).clone()));
            Ok(Type::arrow(
                (**param_ty).clone(),
                ty_of(&ctx, &**body_expr)?,
            ))
        }
    }
}

// Kinding

pub fn simplify_ty<N: Name>(ctx: &Ctx<N>, ty: &binary::Type<N>) -> binary::Type<N> {
    use syntax::binary::Type;

    fn compute_ty<N: Name>(ctx: &Ctx<N>, ty: &binary::Type<N>) -> Option<binary::Type<N>> {
        match *ty {
            Type::Var(Var::Bound(Named(_, i))) => match ctx.lookup_ty_def(i) {
                Ok(Named(_, def_ty)) => Some(def_ty.clone()),
                Err(_) => None,
            },
            Type::App(ref fn_ty, ref arg_ty) => match **fn_ty {
                Type::Abs(_, ref body_ty) => {
                    // FIXME: Avoid clone
                    let mut body = (**body_ty).clone();
                    body.instantiate(arg_ty);
                    Some(body)
                }
                _ => None,
            },
            _ => None,
        }
    }

    let ty = match *ty {
        Type::App(ref fn_ty, _) => simplify_ty(ctx, &**fn_ty),
        // FIXME: Avoid clone
        _ => ty.clone(),
    };

    match compute_ty(ctx, &ty) {
        Some(ty) => simplify_ty(ctx, &ty),
        None => ty,
    }
}

/// An error that was encountered during kind checking
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KindError<N> {
    /// A variable of the requested name was not bound in this scope
    UnboundVariable { ty: binary::Type<N>, name: N },
    /// Variable bound in the context was not at the type level
    TypeBindingExpected {
        ty: binary::Type<N>,
        found: Named<N, Binding<N>>,
    },
    /// One kind was expected, but another was found
    Mismatch {
        ty: binary::Type<N>,
        expected: binary::Kind,
        found: binary::Kind,
    },
    /// No host representation was found for this type
    NoReprForType { ty: binary::Type<N> },
    /// A type error
    Type(TypeError<N>),
}

impl<N> From<TypeError<N>> for KindError<N> {
    fn from(src: TypeError<N>) -> KindError<N> {
        KindError::Type(src)
    }
}

/// Returns the kind of a binary type, checking that it is properly formed in
/// the environment
pub fn kind_of<N: Name>(ctx: &Ctx<N>, ty: &binary::Type<N>) -> Result<binary::Kind, KindError<N>> {
    use syntax::binary::{Kind, Type, TypeConst};

    match *ty {
        // Variables
        Type::Var(Var::Free(ref x)) => Err(KindError::UnboundVariable {
            ty: ty.clone(),
            name: x.clone(),
        }),
        Type::Var(Var::Bound(Named(_, i))) => match ctx.lookup_kind(i) {
            Ok(Named(_, kind)) => Ok(kind.clone()),
            Err(Named(name, binding)) => Err(KindError::TypeBindingExpected {
                ty: ty.clone(),
                found: Named(name.clone(), binding.clone()),
            }),
        },

        // Bit type
        Type::Const(TypeConst::Bit) => Ok(Kind::Type),

        // Array types
        Type::Array(ref elem_ty, ref size_expr) => {
            expect_ty_kind(ctx, &**elem_ty)?;
            expect_ty(ctx, &**size_expr, host::Type::int())?;

            Ok(Kind::Type)
        }

        // Conditional types
        Type::Cond(ref ty, ref pred_expr) => {
            expect_ty_kind(ctx, &**ty)?;
            expect_ty(
                ctx,
                &**pred_expr,
                host::Type::arrow(ty.repr().unwrap(), host::Type::bool()),
            )?;

            Ok(Kind::Type)
        }

        // Interpreted types
        Type::Interp(ref ty, ref conv_expr, ref host_ty) => {
            expect_ty_kind(ctx, &**ty)?;
            expect_ty(
                ctx,
                &**conv_expr,
                host::Type::arrow(ty.repr().unwrap(), host_ty.clone()),
            )?;

            Ok(Kind::Type)
        }

        // Type abstraction
        Type::Abs(Named(ref name, ref param_kind), ref body_ty) => {
            // FIXME: avoid cloning the environment
            let mut ctx = ctx.clone();
            ctx.extend(name.clone(), Binding::Type(param_kind.clone()));
            Ok(Kind::arrow(param_kind.clone(), kind_of(&ctx, &**body_ty)?))
        }

        // Union types
        Type::Union(ref tys) => {
            for ty in tys {
                expect_ty_kind(ctx, ty)?;
            }

            Ok(Kind::Type)
        }

        // Struct type
        Type::Struct(ref fields) => {
            // FIXME: avoid cloning the environment
            let mut ctx = ctx.clone();

            for field in fields {
                expect_ty_kind(&ctx, &field.value)?;

                let field_ty = simplify_ty(&ctx, &field.value);
                let repr_ty = field_ty.repr().map_err(|_| {
                    KindError::NoReprForType {
                        ty: field_ty.clone(),
                    }
                })?;
                ctx.extend(field.name.clone(), Binding::Expr(repr_ty));
            }

            Ok(Kind::Type)
        }

        // Type application
        Type::App(ref fn_ty, ref arg_ty) => {
            match kind_of(ctx, &**fn_ty)? {
                Kind::Type => Err(KindError::Mismatch {
                    ty: (**fn_ty).clone(),
                    found: Kind::Type,
                    // FIXME: Kind of args are unknown at this point - therefore
                    // they shouldn't be `Kind::Type`!
                    expected: Kind::arrow(Kind::Type, Kind::Type),
                }),
                Kind::Arrow(param_kind, ret_kind) => {
                    expect_kind(ctx, &**arg_ty, *param_kind)?;
                    Ok(*ret_kind)
                }
            }
        }
    }
}

pub fn check_defs<'a, N: 'a + Name, Defs>(defs: Defs) -> Result<(), KindError<N>>
where
    Defs: IntoIterator<Item = &'a Definition<N>>,
{
    let mut ctx = Ctx::new();
    // We maintain a list of the seen definition names. This will allow us to
    // recover the index of these variables as we abstract later definitions...
    let mut seen_names = Vec::new();

    for def in defs {
        let mut def_ty = def.ty.clone();

        // Kind of ugly and inefficient - can't we just substitute directly?
        // Should handle mutually recursive bindings as well...

        for (level, name) in seen_names.iter().rev().enumerate() {
            def_ty.abstract_name_at(name, level as u32);
        }

        for (i, _) in seen_names.iter().enumerate() {
            let Named(_, ty) = ctx.lookup_ty_def(i as u32).unwrap();
            def_ty.instantiate(ty);
        }

        let def_kind = kind_of(&ctx, &*def_ty)?;
        ctx.extend(def.name.clone(), Binding::TypeDef(*def_ty, def_kind));

        // Record that the definition has been 'seen'
        seen_names.push(def.name.clone());
    }

    Ok(())
}

// Expectations

fn expect_ty<N: Name>(
    ctx: &Ctx<N>,
    expr: &host::Expr<N>,
    expected: host::Type<N>,
) -> Result<host::Type<N>, TypeError<N>> {
    let found = ty_of(ctx, expr)?;

    if found == expected {
        Ok(found)
    } else {
        Err(TypeError::Mismatch {
            expr: expr.clone(),
            expected,
            found,
        })
    }
}

fn expect_kind<N: Name>(
    ctx: &Ctx<N>,
    ty: &binary::Type<N>,
    expected: binary::Kind,
) -> Result<binary::Kind, KindError<N>> {
    let found = kind_of(ctx, ty)?;

    if found == expected {
        Ok(found)
    } else {
        Err(KindError::Mismatch {
            ty: ty.clone(),
            expected,
            found,
        })
    }
}

fn expect_ty_kind<N: Name>(ctx: &Ctx<N>, ty: &binary::Type<N>) -> Result<(), KindError<N>> {
    expect_kind(ctx, ty, binary::Kind::Type).map(|_| ())
}
