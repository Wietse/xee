use xee_xpath_ast::ast;

use xee_interpreter::interpreter::instruction::{
    encode_instruction, instruction_size, Instruction,
};
use xee_interpreter::{context, error, function, interpreter, sequence, span, xml};

// Every index and jump displacement in the bytecode is a 16-bit field.
// One that does not fit is an implementation limit (XPDY0130), refused
// with the span of the expression that needed it, never truncated.
fn limit_exceeded(span: span::SourceSpan) -> error::SpannedError {
    error::Error::XPDY0130.with_span(span)
}

use crate::ir;

#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ForwardJumpRef(usize);

#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct BackwardJumpRef(usize);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum JumpCondition {
    Always,
    True,
    False,
}

pub struct FunctionBuilder<'a> {
    program: &'a mut interpreter::Program,
    compiled: Vec<u8>,
    spans: Vec<span::SourceSpan>,
    constants: Vec<sequence::Sequence>,
    steps: Vec<xml::Step>,
    cast_types: Vec<function::CastType>,
    sequence_types: Vec<ast::SequenceType>,
    closure_names: Vec<ir::Name>,
}

impl<'a> FunctionBuilder<'a> {
    pub fn new(program: &'a mut interpreter::Program) -> Self {
        FunctionBuilder {
            program,
            compiled: Vec::new(),
            spans: Vec::new(),
            constants: Vec::new(),
            steps: Vec::new(),
            cast_types: Vec::new(),
            sequence_types: Vec::new(),
            closure_names: Vec::new(),
        }
    }

    pub(crate) fn static_context(&self) -> &context::StaticContext {
        self.program.static_context()
    }

    pub(crate) fn emit(&mut self, instruction: Instruction, span: span::SourceSpan) {
        for _ in 0..instruction_size(&instruction) {
            self.spans.push(span);
        }
        encode_instruction(instruction, &mut self.compiled);
    }

    pub(crate) fn emit_constant(
        &mut self,
        constant: sequence::Sequence,
        span: span::SourceSpan,
    ) -> error::SpannedResult<()> {
        let constant_id = u16::try_from(self.constants.len()).map_err(|_| limit_exceeded(span))?;
        self.constants.push(constant);
        self.emit(Instruction::Const(constant_id), span);
        Ok(())
    }

    pub(crate) fn add_closure_name(
        &mut self,
        name: &ir::Name,
        span: span::SourceSpan,
    ) -> error::SpannedResult<u16> {
        let index = match self.closure_names.iter().position(|n| n == name) {
            Some(index) => index,
            None => {
                self.closure_names.push(name.clone());
                self.closure_names.len() - 1
            }
        };
        u16::try_from(index).map_err(|_| limit_exceeded(span))
    }

    pub(crate) fn add_step(
        &mut self,
        step: xml::Step,
        span: span::SourceSpan,
    ) -> error::SpannedResult<u16> {
        let step_id = u16::try_from(self.steps.len()).map_err(|_| limit_exceeded(span))?;
        self.steps.push(step);
        Ok(step_id)
    }

    pub(crate) fn add_cast_type(
        &mut self,
        cast_type: function::CastType,
        span: span::SourceSpan,
    ) -> error::SpannedResult<u16> {
        let cast_type_id =
            u16::try_from(self.cast_types.len()).map_err(|_| limit_exceeded(span))?;
        self.cast_types.push(cast_type);
        Ok(cast_type_id)
    }

    pub(crate) fn add_sequence_type(
        &mut self,
        sequence_type: ast::SequenceType,
        span: span::SourceSpan,
    ) -> error::SpannedResult<u16> {
        let sequence_type_id =
            u16::try_from(self.sequence_types.len()).map_err(|_| limit_exceeded(span))?;
        self.sequence_types.push(sequence_type);
        Ok(sequence_type_id)
    }

    pub(crate) fn loop_start(&self) -> BackwardJumpRef {
        BackwardJumpRef(self.compiled.len())
    }

    pub(crate) fn emit_jump_backward(
        &mut self,
        jump_ref: BackwardJumpRef,
        condition: JumpCondition,
        span: span::SourceSpan,
    ) -> error::SpannedResult<()> {
        let current = self.compiled.len() + 3;
        if jump_ref.0 > current {
            panic!("cannot jump forward");
        }
        // The displacement is an `i16`. Checking it against `u16::MAX`, as
        // this used to, let a displacement between 32,768 and 65,535 wrap
        // to a jump the other way.
        let offset = i16::try_from(current - jump_ref.0).map_err(|_| limit_exceeded(span))?;

        match condition {
            JumpCondition::True => self.emit(Instruction::JumpIfTrue(-offset), span),
            JumpCondition::False => self.emit(Instruction::JumpIfFalse(-offset), span),
            JumpCondition::Always => self.emit(Instruction::Jump(-offset), span),
        }
        Ok(())
    }

    pub(crate) fn emit_jump_forward(
        &mut self,
        condition: JumpCondition,
        span: span::SourceSpan,
    ) -> ForwardJumpRef {
        let index = self.compiled.len();
        match condition {
            JumpCondition::True => self.emit(Instruction::JumpIfTrue(0), span),
            JumpCondition::False => self.emit(Instruction::JumpIfFalse(0), span),
            JumpCondition::Always => self.emit(Instruction::Jump(0), span),
        }
        ForwardJumpRef(index)
    }

    pub(crate) fn patch_jump(
        &mut self,
        jump_ref: ForwardJumpRef,
        span: span::SourceSpan,
    ) -> error::SpannedResult<()> {
        let current = self.compiled.len();
        if jump_ref.0 > current {
            panic!("can only patch forward jumps");
        }
        // 3 for the size of the jump. The displacement is decoded as an
        // `i16`: one above 32,767 written here was read back as a jump the
        // other way (an `else if` chain 2,048 long hit it).
        let offset = i16::try_from(current - jump_ref.0 - 3).map_err(|_| limit_exceeded(span))?;
        let offset_bytes = offset.to_le_bytes();
        self.compiled[jump_ref.0 + 1] = offset_bytes[0];
        self.compiled[jump_ref.0 + 2] = offset_bytes[1];
        Ok(())
    }

    pub(crate) fn finish(
        mut self,
        name: String,
        function_definition: &ir::FunctionDefinition,
        span: span::SourceSpan,
    ) -> error::SpannedResult<function::InlineFunction> {
        if let Some(return_type) = &function_definition.return_type {
            let sequence_type_id = self.add_sequence_type(return_type.clone(), span)?;
            self.emit(Instruction::ReturnConvert(sequence_type_id), span);
        }
        self.emit(Instruction::Return, span);
        Ok(function::InlineFunction {
            name,
            signature: function_definition.signature(),
            chunk: self.compiled,
            spans: self.spans,
            closure_names: self.closure_names,
            constants: self.constants,
            steps: self.steps,
            cast_types: self.cast_types,
            sequence_types: self.sequence_types,
        })
    }

    pub(crate) fn builder(&mut self) -> FunctionBuilder<'_> {
        FunctionBuilder::new(self.program)
    }

    pub(crate) fn add_function(
        &mut self,
        function: function::InlineFunction,
        span: span::SourceSpan,
    ) -> error::SpannedResult<function::InlineFunctionId> {
        // a function's id is a `u16` in the `Closure` instruction
        if self.program.function_count() > usize::from(u16::MAX) {
            return Err(limit_exceeded(span));
        }
        Ok(self.program.add_function(function))
    }
}

#[cfg(test)]
mod tests {
    use xee_interpreter::interpreter::instruction::decode_instructions;

    use super::*;

    fn program() -> interpreter::Program {
        interpreter::Program::new(context::StaticContext::default(), (0..0).into())
    }

    fn assert_limit<T: std::fmt::Debug>(result: error::SpannedResult<T>) {
        assert_eq!(result.unwrap_err().error, error::Error::XPDY0130);
    }

    // Constants, steps, cast types and sequence types are numbered by a
    // `u16`: 65,536 of each fit, and one more is refused.
    #[test]
    fn a_table_of_65536_entries_fits_and_one_more_does_not() {
        let mut program = program();
        let mut builder = FunctionBuilder::new(&mut program);
        let span = (0..0).into();
        let step = xml::Step {
            axis: ast::Axis::Child,
            node_test: ast::NodeTest::KindTest(ast::KindTest::Any),
        };
        let cast_type = function::CastType {
            xs: xee_schema_type::Xs::String,
            empty_sequence_allowed: false,
        };
        for i in 0..=u16::MAX {
            builder
                .emit_constant(sequence::Sequence::default(), span)
                .unwrap();
            assert_eq!(builder.add_step(step.clone(), span).unwrap(), i);
            assert_eq!(builder.add_cast_type(cast_type, span).unwrap(), i);
            assert_eq!(
                builder
                    .add_sequence_type(ast::SequenceType::Empty, span)
                    .unwrap(),
                i
            );
        }
        assert_eq!(
            decode_instructions(&builder.compiled[builder.compiled.len() - 3..]),
            [Instruction::Const(u16::MAX)]
        );
        assert_limit(builder.emit_constant(sequence::Sequence::default(), span));
        assert_limit(builder.add_step(step, span));
        assert_limit(builder.add_cast_type(cast_type, span));
        assert_limit(builder.add_sequence_type(ast::SequenceType::Empty, span));
    }

    // A closed-over variable's index is a `u16` in `ClosureVar`: the
    // 65,536th name fits, and one more is refused. A name already there is
    // found, not added again. The table is filled directly, as adding
    // 65,536 names one by one, each searched for first, is quadratic.
    #[test]
    fn closure_name_65536_fits_and_one_more_does_not() {
        let mut program = program();
        let mut builder = FunctionBuilder::new(&mut program);
        let span = (0..0).into();
        let name = |i: usize| ir::Name::new(format!("v{i}"));
        builder.closure_names = (0..usize::from(u16::MAX)).map(name).collect();
        assert_eq!(
            builder.add_closure_name(&name(65_535), span).unwrap(),
            u16::MAX
        );
        assert_eq!(builder.add_closure_name(&name(0), span).unwrap(), 0);
        assert_limit(builder.add_closure_name(&name(65_536), span));
    }

    // A function's id is a `u16` in the `Closure` instruction: the
    // 65,536th function fits, the 65,537th is refused.
    #[test]
    fn function_65536_fits_and_one_more_does_not() {
        let mut program = program();
        let span = (0..0).into();
        let definition = ir::FunctionDefinition {
            params: vec![],
            return_type: None,
            body: Box::new(xee_xpath_ast::span::Spanned::new(
                ir::Expr::Atom(xee_xpath_ast::span::Spanned::new(
                    ir::Atom::Const(ir::Const::EmptySequence),
                    (0..0).into(),
                )),
                (0..0).into(),
            )),
        };
        let function = FunctionBuilder::new(&mut program)
            .finish(String::new(), &definition, span)
            .unwrap();
        program.functions = vec![function.clone(); usize::from(u16::MAX)];
        FunctionBuilder::new(&mut program)
            .add_function(function.clone(), span)
            .unwrap();
        assert_eq!(program.function_count(), 65_536);
        assert_limit(FunctionBuilder::new(&mut program).add_function(function, span));
    }

    // A jump's displacement is an `i16`. These place it exactly at the
    // bound: 32,767 is encoded, 32,768 is an implementation limit.

    #[test]
    fn a_forward_jump_of_32767_fits_and_32768_does_not() {
        for (filler, fits) in [(32_767, true), (32_768, false)] {
            let mut program = program();
            let mut builder = FunctionBuilder::new(&mut program);
            let span = (0..0).into();
            let jump = builder.emit_jump_forward(JumpCondition::Always, span);
            for _ in 0..filler {
                builder.emit(Instruction::Pop, span);
            }
            match builder.patch_jump(jump, span) {
                Ok(()) => {
                    assert!(fits, "a displacement of {filler} was encoded");
                    assert_eq!(
                        decode_instructions(&builder.compiled[..3]),
                        [Instruction::Jump(32_767)]
                    );
                }
                Err(e) => {
                    assert!(!fits, "a displacement of {filler} was refused");
                    assert_eq!(e.error, error::Error::XPDY0130);
                }
            }
        }
    }

    #[test]
    fn a_backward_jump_of_32767_fits_and_32768_does_not() {
        // the displacement counts the jump's own 3 bytes
        for (filler, fits) in [(32_764, true), (32_765, false)] {
            let mut program = program();
            let mut builder = FunctionBuilder::new(&mut program);
            let span = (0..0).into();
            let start = builder.loop_start();
            for _ in 0..filler {
                builder.emit(Instruction::Pop, span);
            }
            match builder.emit_jump_backward(start, JumpCondition::Always, span) {
                Ok(()) => {
                    assert!(fits, "a displacement of {} was encoded", filler + 3);
                    assert_eq!(
                        decode_instructions(&builder.compiled[filler..]),
                        [Instruction::Jump(-32_767)]
                    );
                }
                Err(e) => {
                    assert!(!fits, "a displacement of {} was refused", filler + 3);
                    assert_eq!(e.error, error::Error::XPDY0130);
                }
            }
        }
    }
}
