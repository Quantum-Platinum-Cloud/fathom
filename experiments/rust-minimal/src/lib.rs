//! Experimenting with a simple implementation of type theory elaboration
//!
//! Based on [Andras Korvacs' example type checker][02-typecheck-closures-debruijn].
//! We adapt it to Rust, using arenas for allocating source terms, and
//! reference-counting in values.
//!
//! [02-typecheck-closures-debruijn]: https://github.com/AndrasKovacs/elaboration-zoo/tree/master/02-typecheck-closures-debruijn

// TODO:
//
// - language features
//   - [x] let expressions
//   - [x] dependent functions
//   - [ ] dependent records
//   - [ ] top-level items
//   - [ ] recursive definitions
//   - [ ] binary format descriptions
//     - [ ] error formats
//     - [ ] map formats
//     - [ ] pure formats
//     - [ ] bind formats
// - implementation
//   - [x] command line interface
//   - [x] parser
//   - [ ] source location tracking
//   - [x] string interning
//   - [x] arena allocation
//   - [x] normalisation-by-evaluation
//      - [ ] improved closure representation
//   - [x] elaborator
//     - [ ] error recovery
//   - [ ] distiller
//   - [ ] pretty printing
//   - [ ] integration tests

/// Interned strings.
pub type StringId = string_interner::symbol::SymbolU16;

/// String interner.
pub type StringInterner = string_interner::StringInterner<
    StringId,
    string_interner::backend::BucketBackend<StringId>,
    std::hash::BuildHasherDefault<fxhash::FxHasher32>,
>;

/// De-bruijn index
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalVar(u16);

/// De-bruijn level
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct GlobalVar(u16);

/// Length of the environment.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct EnvLen(u16);

impl EnvLen {
    fn local_to_global(self, local: LocalVar) -> Option<GlobalVar> {
        Some(GlobalVar(self.0.checked_sub(local.0)?.checked_sub(1)?))
    }

    fn global_to_local(self, global: GlobalVar) -> Option<LocalVar> {
        Some(LocalVar(self.0.checked_sub(global.0)?.checked_sub(1)?))
    }

    fn next_global(self) -> GlobalVar {
        GlobalVar(self.0)
    }

    fn add_param(self) -> EnvLen {
        EnvLen(self.0 + 1) // FIXME: overflow?
    }
}

/// A generic environment
#[derive(Clone)]
pub struct Env<Entry> {
    // TODO: figure out a better representation for this:
    //
    // - should avoid clones if possible
    // - allow for fast, in-place pushes on the end of an immutable list?
    // - maybe some sort of chunked tree structure?
    // - could also use a linked list but idk
    // - `im::Vector` is ergonomic, but a bit chonky
    entries: Vec<Entry>,
}

impl<Entry> Env<Entry> {
    fn new() -> Env<Entry> {
        Env {
            entries: Vec::new(),
        }
    }

    fn len(&self) -> EnvLen {
        EnvLen(self.entries.len() as u16)
    }

    fn get_global(&self, global: GlobalVar) -> Option<&Entry> {
        self.entries.get(global.0 as usize)
    }

    fn get_local(&self, local: LocalVar) -> Option<&Entry> {
        self.get_global(self.len().local_to_global(local)?)
    }

    fn push_entry(&mut self, entry: Entry) {
        // FIXME: check if `self.entries.len()` exceeds `u16::MAX`
        self.entries.push(entry);
    }

    fn pop_entry(&mut self) {
        self.entries.pop();
    }
}

/// Core language.
pub mod core {
    use crate::{LocalVar, StringId};

    pub type TermRef<'arena> = &'arena Term<'arena>;

    /// Core language terms.
    pub enum Term<'arena> {
        /// Variable occurrences.
        Var(LocalVar),
        /// Let expressions.
        Let(StringId, TermRef<'arena>, TermRef<'arena>, TermRef<'arena>),
        /// The type of types.
        Universe,
        /// Dependent function types.
        ///
        /// Also known as: pi types, dependent product types.
        FunType(StringId, TermRef<'arena>, TermRef<'arena>),
        /// Function introductions.
        ///
        /// Also known as: lambda expressions, anonymous functions.
        FunIntro(StringId, TermRef<'arena>),
        /// Function eliminations.
        ///
        /// Also known as: function applications.
        FunElim(TermRef<'arena>, TermRef<'arena>),
        // RecordType(&'arena [StringId], &'arena [Term<'arena>]),
        // RecordIntro(&'arena [StringId], &'arena [Term<'arena>]),
        // RecordElim(TermRef<'arena>, StringId),
    }

    /// The semantics of the core language, implemented through the use of
    /// normalization-by-evaluation.
    pub mod semantics {
        use std::sync::Arc;

        use typed_arena::Arena;

        use crate::core::{Term, TermRef};
        use crate::{Env, EnvLen, GlobalVar, StringId};

        pub type ValueEnv<'arena> = Env<Arc<Value<'arena>>>;

        /// Values in weak-head-normal form.
        #[derive(Clone)]
        pub enum Value<'arena> {
            /// A value whose computation has stopped as a result of trying to
            /// [evaluate][`eval`] an open [term][`Term`].
            Stuck(GlobalVar, Vec<Elim<'arena>>),
            /// Universes.
            Universe,
            /// Dependent function types.
            FunType(StringId, Arc<Value<'arena>>, Closure<'arena>),
            /// Function introductions.
            FunIntro(StringId, Closure<'arena>),
            // RecordType(&'arena [StringId], Telescope<'arena>),
            // RecordIntro(&'arena [StringId], Telescope<'arena>),
        }

        /// A pending elimination to be reduced if the [head][`Head`] of a
        /// [stuck value][`Value::Stuck`] becomes known.
        #[derive(Clone)]
        pub enum Elim<'arena> {
            /// Function eliminations.
            Fun(Arc<Value<'arena>>),
            // Record(StringId),
        }

        /// A closure is a term and a captured environment that will be later
        /// evaluated
        /// that will be later evaluated in the presence of an input expression.
        #[derive(Clone)]
        pub struct Closure<'arena> {
            /// Captured environment.
            env: ValueEnv<'arena>,
            /// The body expression.
            ///
            /// This can be evaluated using the captured environment with an
            /// expression pushed onto it.
            body_expr: TermRef<'arena>,
        }

        impl<'arena> Closure<'arena> {
            pub fn new(env: ValueEnv<'arena>, body_expr: TermRef<'arena>) -> Closure<'arena> {
                Closure { env, body_expr }
            }

            /// Apply an input to the closure.
            pub fn apply(
                &self,
                input_expr: Arc<Value<'arena>>,
            ) -> Result<Arc<Value<'arena>>, EvalError> {
                let mut env = self.env.clone(); // FIXME: ValueEnv::clone
                env.push_entry(input_expr); // Add the input expression to the environment
                eval(&mut env, self.body_expr) // Evaluate the body expression
            }
        }

        // TODO: include stack trace(??)
        #[derive(Clone, Debug)]
        pub enum EvalError {
            MisboundLocal,
            InvalidFunctionElimHead,
        }

        pub fn normalise<'in_arena, 'out_arena>(
            arena: &'out_arena Arena<Term<'out_arena>>,
            env: &mut ValueEnv<'in_arena>,
            term: &Term<'in_arena>,
        ) -> Result<Term<'out_arena>, EvalError> {
            readback(arena, env.len(), eval(env, term)?.as_ref())
        }

        pub fn eval<'arena>(
            env: &mut ValueEnv<'arena>,
            term: &Term<'arena>,
        ) -> Result<Arc<Value<'arena>>, EvalError> {
            match term {
                Term::Var(local) => match env.get_local(*local) {
                    Some(value) => Ok(value.clone()),
                    None => Err(EvalError::MisboundLocal),
                },
                Term::Let(_, _, expr, body_expr) => {
                    let expr = eval(env, expr)?;
                    env.push_entry(expr);
                    let body_expr = eval(env, body_expr);
                    env.pop_entry();
                    body_expr
                }
                Term::Universe => Ok(Arc::new(Value::Universe)),
                Term::FunType(name, input_type, output_type) => {
                    let input_type = eval(env, input_type)?;
                    let output_type = Closure::new(env.clone(), output_type); // FIXME: ValueEnv::clone
                    Ok(Arc::new(Value::FunType(*name, input_type, output_type)))
                }
                Term::FunIntro(name, output_expr) => {
                    let output_expr = Closure::new(env.clone(), output_expr); // FIXME: ValueEnv::clone
                    Ok(Arc::new(Value::FunIntro(*name, output_expr)))
                }
                Term::FunElim(head_expr, input_expr) => {
                    let head_expr = eval(env, head_expr)?;
                    let input_expr = eval(env, input_expr)?;
                    fun_elim(head_expr, input_expr)
                }
            }
        }

        pub fn fun_elim<'arena>(
            mut head_expr: Arc<Value<'arena>>,
            input_expr: Arc<Value<'arena>>,
        ) -> Result<Arc<Value<'arena>>, EvalError> {
            match Arc::make_mut(&mut head_expr) {
                Value::FunIntro(_, output_expr) => output_expr.apply(input_expr),
                Value::Stuck(_, elims) => {
                    elims.push(Elim::Fun(input_expr));
                    Ok(head_expr)
                }
                _ => Err(EvalError::InvalidFunctionElimHead),
            }
        }

        /// Read a [value][`Value`] back into a [term][`Term`].
        pub fn readback<'in_arena, 'out_arena>(
            arena: &'out_arena Arena<Term<'out_arena>>,
            env_len: EnvLen,
            value: &Value<'in_arena>,
        ) -> Result<Term<'out_arena>, EvalError> {
            match value {
                Value::Stuck(global, elims) => {
                    let mut head_expr = Term::Var(env_len.global_to_local(*global).unwrap()); // FIXME: Unwrap
                    for elim in elims {
                        head_expr = match elim {
                            Elim::Fun(input_expr) => {
                                let input_expr = readback(arena, env_len, input_expr)?;
                                Term::FunElim(arena.alloc(head_expr), arena.alloc(input_expr))
                            }
                        };
                    }
                    Ok(head_expr)
                }
                Value::Universe => Ok(Term::Universe),
                Value::FunType(name, input_type, output_type) => {
                    let input_type = readback(arena, env_len, input_type)?;
                    let var = Arc::new(Value::Stuck(env_len.next_global(), Vec::new()));
                    let output_type = output_type.apply(var)?;
                    let output_type = readback(arena, env_len.add_param(), &output_type)?;

                    Ok(Term::FunType(
                        *name,
                        arena.alloc(input_type),
                        arena.alloc(output_type),
                    ))
                }
                Value::FunIntro(name, output_expr) => {
                    let var = Arc::new(Value::Stuck(env_len.next_global(), Vec::new()));
                    let output_expr = output_expr.apply(var)?;
                    let output_expr = readback(arena, env_len.add_param(), &output_expr)?;

                    Ok(Term::FunIntro(*name, arena.alloc(output_expr)))
                }
            }
        }

        /// Check that one value is [computationally equal] to another value.
        ///
        /// This is sometimes referred to as 'conversion checking', or checking
        /// for 'definitional equality'.
        ///
        /// [computationally equal]: https://ncatlab.org/nlab/show/equality#computational_equality
        pub fn is_equal(
            env_len: EnvLen,
            value0: &Arc<Value<'_>>,
            value1: &Arc<Value<'_>>,
        ) -> Result<bool, EvalError> {
            match (value0.as_ref(), value1.as_ref()) {
                (Value::Stuck(global0, elims0), Value::Stuck(global1, elims1)) => {
                    if global0 != global1 || elims0.len() != elims1.len() {
                        return Ok(false);
                    }
                    for (elim0, elim1) in Iterator::zip(elims0.iter(), elims1.iter()) {
                        match (elim0, elim1) {
                            (Elim::Fun(input_expr0), Elim::Fun(input_expr1))
                                if is_equal(env_len, input_expr0, input_expr1)? => {}
                            (_, _) => return Ok(false),
                        }
                    }
                    Ok(true)
                }
                (Value::Universe, Value::Universe) => Ok(true),
                (
                    Value::FunType(_, input_type0, output_type0),
                    Value::FunType(_, input_type1, output_type1),
                ) => Ok(is_equal(env_len, input_type0, input_type1)? && {
                    let var = Arc::new(Value::Stuck(env_len.next_global(), Vec::new()));
                    let output_type0 = output_type0.apply(var.clone())?;
                    let output_type1 = output_type1.apply(var)?;

                    is_equal(env_len.add_param(), &output_type0, &output_type1)?
                }),
                (Value::FunIntro(_, output_expr0), Value::FunIntro(_, output_expr1)) => {
                    let var = Arc::new(Value::Stuck(env_len.next_global(), Vec::new()));
                    let output_expr0 = output_expr0.apply(var.clone())?;
                    let output_expr1 = output_expr1.apply(var)?;

                    is_equal(env_len.add_param(), &output_expr0, &output_expr1)
                }

                // Eta-conversion
                (Value::FunIntro(_, output_expr), _) => {
                    let var = Arc::new(Value::Stuck(env_len.next_global(), Vec::new()));
                    let value0 = output_expr.apply(var.clone())?;
                    let value1 = fun_elim(value1.clone(), var)?;

                    is_equal(env_len.add_param(), &value0, &value1)
                }
                (_, Value::FunIntro(_, output_expr)) => {
                    let var = Arc::new(Value::Stuck(env_len.next_global(), Vec::new()));
                    let value0 = fun_elim(value0.clone(), var.clone())?;
                    let value1 = output_expr.apply(var)?;

                    is_equal(env_len.add_param(), &value0, &value1)
                }

                (_, _) => Ok(false),
            }
        }
    }
}

/// Surface language.
pub mod surface {
    use lalrpop_util::lalrpop_mod;
    use typed_arena::Arena;

    use crate::{StringId, StringInterner};

    pub mod lexer {
        use logos::Logos;

        #[derive(Clone, Debug, Logos)]
        pub enum Token<'source> {
            #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
            Ident(&'source str),

            #[token("fun")]
            KeywordFun,
            #[token("let")]
            KeywordLet,
            #[token("in")]
            KeywordIn,

            #[token(":")]
            Colon,
            #[token("=")]
            Equals,
            #[token("=>")]
            EqualsGreater,
            #[token("->")]
            HyphenGreater,

            #[token("(")]
            OpenParen,
            #[token(")")]
            CloseParen,

            #[error]
            #[regex(r"\p{Whitespace}", logos::skip)]
            #[regex(r"//(.*)\n", logos::skip)]
            Error,
        }

        pub type Spanned<Tok, Loc> = (Loc, Tok, Loc);

        pub fn tokens<'source>(
            source: &'source str,
        ) -> impl 'source + Iterator<Item = Result<Spanned<Token<'source>, usize>, ()>> {
            Token::lexer(source)
                .spanned()
                .map(|(token, range)| match token {
                    Token::Error => Err(()),
                    token => Ok((range.start, token, range.end)),
                })
        }
    }

    lalrpop_mod!(grammar);

    pub type TermRef<'arena> = &'arena Term<'arena>;

    pub enum Term<'arena> {
        Var(StringId),
        Let(StringId, TermRef<'arena>, TermRef<'arena>, TermRef<'arena>),
        Universe,
        FunType(StringId, TermRef<'arena>, TermRef<'arena>),
        FunIntro(StringId, TermRef<'arena>),
        FunElim(TermRef<'arena>, TermRef<'arena>),
    }

    // TODO: Convert to an internal error message
    pub type ParseError<'source> = lalrpop_util::ParseError<usize, lexer::Token<'source>, ()>;

    impl<'arena> Term<'arena> {
        /// Parse a term from the `source` string, interning strings to the
        /// supplied `interner` and allocating nodes to the `arena`.
        pub fn parse<'source>(
            interner: &mut StringInterner,
            arena: &'arena Arena<Term<'arena>>,
            source: &'source str,
        ) -> Result<Term<'arena>, ParseError<'source>> {
            grammar::TermParser::new().parse(interner, arena, lexer::tokens(source))
        }
    }

    // TODO: pretty print terms
}

/// Bidirectional elaboration of the surface language into the core language.
pub mod elaboration {
    use std::convert::TryInto;
    use std::sync::Arc;
    use typed_arena::Arena;

    use crate::core::semantics::{self, Value, ValueEnv};
    use crate::{core, surface, LocalVar, StringId};

    /// Elaboration context.
    pub struct Context<'arena> {
        /// Arena used for storing elaborated terms.
        arena: &'arena Arena<core::Term<'arena>>,
        /// Type environment.
        ///
        /// Name-type pairs will be added here.
        types: Vec<(StringId, Arc<Value<'arena>>)>,
        /// Value environment.
        ///
        /// The values stored in this environment correspond to the the types in
        /// the type environment.
        env: ValueEnv<'arena>,
        /// Diagnostic messages encountered during elaboration.
        messages: Vec<String>,
    }

    impl<'arena> Context<'arena> {
        /// Construct a new elaboration context, backed by the supplied arena.
        pub fn new(arena: &'arena Arena<core::Term<'arena>>) -> Context<'arena> {
            Context {
                arena,
                types: Vec::new(),
                env: ValueEnv::new(),
                messages: Vec::new(),
            }
        }

        fn push_entry(
            &mut self,
            name: StringId,
            value: Arc<Value<'arena>>,
            r#type: Arc<Value<'arena>>,
        ) {
            self.types.push((name, r#type));
            self.env.push_entry(value);
        }

        fn push_param(&mut self, name: StringId, r#type: Arc<Value<'arena>>) -> Arc<Value<'arena>> {
            let value = Arc::new(Value::Stuck(self.env.len().next_global(), Vec::new()));
            self.push_entry(name, value.clone(), r#type);
            value
        }

        fn pop_entry(&mut self) {
            self.types.pop();
            self.env.pop_entry();
        }

        fn report<T>(&mut self, message: impl Into<String>) -> Option<T> {
            self.messages.push(message.into());
            None
        }

        pub fn normalize<'out_arena>(
            &mut self,
            arena: &'out_arena Arena<core::Term<'out_arena>>,
            term: &core::Term<'arena>,
        ) -> Option<core::Term<'out_arena>> {
            semantics::normalise(arena, &mut self.env, term).ok() // FIXME: record error
        }

        pub fn eval(&mut self, term: &core::Term<'arena>) -> Option<Arc<Value<'arena>>> {
            semantics::eval(&mut self.env, term).ok() // FIXME: record error
        }

        pub fn is_equal(
            &mut self,
            value0: &Arc<Value<'_>>,
            value1: &Arc<Value<'_>>,
        ) -> Option<bool> {
            semantics::is_equal(self.env.len(), value0, value1).ok() // FIXME: record error
        }

        /// Check that a surface term conforms to the given type.
        ///
        /// Returns the elaborated term in the core language.
        pub fn check(
            &mut self,
            surface_term: surface::TermRef<'_>,
            expected_type: &Arc<Value<'arena>>,
        ) -> Option<core::Term<'arena>> {
            match (surface_term, expected_type.as_ref()) {
                (surface::Term::Let(name, def_type, def_expr, body_expr), _) => {
                    let def_type = self.check(def_type, &Arc::new(Value::Universe))?; // FIXME: avoid temporary Arc
                    let def_type_value = self.eval(&def_type)?;

                    let def_expr = self.check(def_expr, &def_type_value)?;
                    let def_expr_value = self.eval(&def_expr)?;

                    self.push_entry(*name, def_expr_value, def_type_value);
                    let body_expr = self.check(body_expr, expected_type)?; // FIXME: pop if error occured
                    self.pop_entry();

                    Some(core::Term::Let(
                        *name,
                        self.arena.alloc(def_expr),
                        self.arena.alloc(def_type),
                        self.arena.alloc(body_expr),
                    ))
                }
                (
                    surface::Term::FunIntro(name, output_expr),
                    Value::FunType(_, input_type, output_type),
                ) => {
                    let input_expr = self.push_param(*name, input_type.clone());
                    let output_type = output_type.apply(input_expr).ok()?; // FIXME: record error
                    let output_expr = self.check(output_expr, &output_type)?;
                    self.pop_entry(); // FIXME: pop if error occurred

                    Some(core::Term::FunIntro(*name, self.arena.alloc(output_expr)))
                }
                (_, _) => match self.synth(surface_term)? {
                    (core_term, synth_type) if self.is_equal(&synth_type, expected_type)? => {
                        Some(core_term)
                    }
                    (_, _) => self.report("error: type mismatch"),
                },
            }
        }

        /// Synthesize the type of the given surface term.
        ///
        /// Returns the elaborated term in the core language and its type.
        pub fn synth(
            &mut self,
            surface_term: surface::TermRef<'_>,
        ) -> Option<(core::Term<'arena>, Arc<Value<'arena>>)> {
            match surface_term {
                surface::Term::Var(var_name) => {
                    for (i, (name, r#type)) in self.types.iter().rev().enumerate() {
                        if name == var_name {
                            return match i.try_into() {
                                Ok(i) => Some((core::Term::Var(LocalVar(i)), r#type.clone())),
                                Err(_) => self.report("bug: local index out of range"),
                            };
                        }
                    }
                    self.report("error: variable out of scope")
                }
                surface::Term::Let(name, def_type, def_expr, body_expr) => {
                    let def_type = self.check(def_type, &Arc::new(Value::Universe))?; // FIXME: avoid temporary Arc
                    let def_type_value = self.eval(&def_type)?;

                    let def_expr = self.check(def_expr, &def_type_value)?;
                    let def_expr_value = self.eval(&def_expr)?;

                    self.push_entry(*name, def_expr_value, def_type_value);
                    let (body_expr, body_type) = self.synth(body_expr)?; // FIXME: pop if error occured
                    self.pop_entry();

                    let r#let = core::Term::Let(
                        *name,
                        self.arena.alloc(def_expr),
                        self.arena.alloc(def_type),
                        self.arena.alloc(body_expr),
                    );

                    Some((r#let, body_type))
                }
                surface::Term::Universe => Some((core::Term::Universe, Arc::new(Value::Universe))),
                surface::Term::FunType(name, input_type, output_type) => {
                    let input_type = self.check(input_type, &Arc::new(Value::Universe))?; // FIXME: avoid temporary Arc
                    let input_type_value = self.eval(&input_type)?;

                    self.push_param(*name, input_type_value);
                    let output_type = self.check(output_type, &Arc::new(Value::Universe))?; // FIXME: avoid temporary Arc
                    self.pop_entry(); // FIXME: pop if error occured

                    let fun_type = core::Term::FunType(
                        *name,
                        self.arena.alloc(input_type),
                        self.arena.alloc(output_type),
                    );

                    Some((fun_type, Arc::new(Value::Universe)))
                }
                surface::Term::FunIntro(_, _) => {
                    self.report("error: ambiguous function introduction")
                }
                surface::Term::FunElim(head_expr, input_expr) => {
                    let (head_expr, head_type) = self.synth(head_expr)?;
                    match head_type.as_ref() {
                        Value::FunType(_, input_type, output_type) => {
                            let input_expr = self.check(input_expr, input_type)?;
                            let input_expr_value = self.eval(&input_expr)?;

                            let output_type = output_type.apply(input_expr_value).ok()?; // FIXME: record error

                            let fun_elim = core::Term::FunElim(
                                self.arena.alloc(head_expr),
                                self.arena.alloc(input_expr),
                            );

                            Some((fun_elim, output_type))
                        }
                        _ => self.report("error: expected a function type"),
                    }
                }
            }
        }
    }
}

pub mod distillation {
    // TODO: distill terms from core to surface
}
