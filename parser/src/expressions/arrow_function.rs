use std::borrow::Cow;

use crate::{tsx_keywords, VariableIdentifier};
use visitable_derive::Visitable;

use crate::{
	errors::parse_lexing_error, functions::FunctionBased, parameters::FunctionParameters,
	tokens::token_as_identifier, ASTNode, Block, BlockId, ChainVariable, Expression, FunctionBase,
	FunctionId, Keyword, Parameter, ParseResult, ParseSettings, Span, TSXKeyword, TSXToken, Token,
	TokenReader, TypeReference, VariableField, VariableId, WithComment,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArrowFunctionBase;

pub type ArrowFunction = FunctionBase<ArrowFunctionBase>;

impl FunctionBased for ArrowFunctionBase {
	type Body = ExpressionOrBlock;
	type Header = Option<Keyword<tsx_keywords::Async>>;
	type Name = ();

	fn get_chain_variable(this: &FunctionBase<Self>) -> ChainVariable {
		ChainVariable::UnderArrowFunction(this.body.get_block_id())
	}

	fn header_and_name_from_reader(
		reader: &mut impl TokenReader<TSXToken, Span>,
		_state: &mut crate::ParsingState,
		_settings: &ParseSettings,
	) -> ParseResult<(Self::Header, Self::Name)> {
		let is_async = if let Some(Token(TSXToken::Keyword(TSXKeyword::Async), _)) = reader.peek() {
			let Token(_, pos) = reader.next().unwrap();
			Some(Keyword::new(pos))
		} else {
			None
		};
		Ok((is_async, ()))
	}

	fn header_and_name_to_string_from_buffer<T: source_map::ToString>(
		buf: &mut T,
		is_async: &Self::Header,
		_name: &Self::Name,
		_settings: &crate::ToStringSettings,
		_depth: u8,
	) {
		if is_async.is_some() {
			buf.push_str("async ")
		}
	}

	fn parameters_from_reader<T: source_map::ToString>(
		reader: &mut impl TokenReader<TSXToken, Span>,
		state: &mut crate::ParsingState,
		settings: &ParseSettings,
	) -> ParseResult<FunctionParameters> {
		match reader.next().ok_or_else(parse_lexing_error)? {
			Token(TSXToken::OpenParentheses, open_paren) => {
				FunctionParameters::from_reader_sub_open_parenthesis(
					reader, state, settings, open_paren,
				)
			}
			// `x` => ...
			token => {
				let (name, position) = token_as_identifier(token, "arrow function parameter")?;
				let parameters = vec![Parameter {
					name: WithComment::None(
						VariableIdentifier::Standard(name, VariableId::new(), position.clone())
							.into(),
					),
					type_reference: None,
				}];
				Ok(FunctionParameters {
					parameters,
					optional_parameters: Vec::new(),
					rest_parameter: None,
					position,
				})
			}
		}
	}

	fn parameters_to_string_from_buffer<T: source_map::ToString>(
		buf: &mut T,
		parameters: &FunctionParameters,
		settings: &crate::ToStringSettings,
		depth: u8,
	) {
		// Use shorthand if one parameter with no declared type
		if let (true, [Parameter { name, .. }]) = (
			parameters.optional_parameters.is_empty() && parameters.rest_parameter.is_none(),
			parameters.parameters.as_slice(),
		) {
			if let VariableField::Name(name, ..) = name.get_ast() {
				buf.push_str(name.as_str());
			} else {
				parameters.to_string_from_buffer(buf, settings, depth);
			}
		} else {
			parameters.to_string_from_buffer(buf, settings, depth);
		}
	}

	fn parameter_body_boundary_token_to_string_from_buffer<T: source_map::ToString>(
		buf: &mut T,
		settings: &crate::ToStringSettings,
	) {
		buf.push_str(if settings.pretty { " => " } else { "=>" });
	}

	fn header_left(header: &Self::Header) -> Option<Cow<Span>> {
		header.as_ref().map(|kw| Cow::Borrowed(&kw.1))
	}
}

impl ArrowFunction {
	pub(crate) fn from_reader_with_first_parameter(
		reader: &mut impl TokenReader<TSXToken, Span>,
		state: &mut crate::ParsingState,
		settings: &ParseSettings,
		first_parameter: (String, Span),
	) -> ParseResult<Self> {
		let parameters = vec![crate::Parameter {
			name: WithComment::None(
				VariableIdentifier::Standard(
					first_parameter.0,
					VariableId::new(),
					first_parameter.1.clone(),
				)
				.into(),
			),
			type_reference: None,
		}];
		reader.expect_next(TSXToken::Arrow)?;
		let body = ExpressionOrBlock::from_reader(reader, state, settings)?;
		let arrow_function = FunctionBase {
			header: None,
			name: (),
			parameters: crate::FunctionParameters {
				parameters,
				optional_parameters: Vec::new(),
				rest_parameter: None,
				position: first_parameter.1,
			},
			return_type: None,
			type_parameters: None,
			body,
			function_id: FunctionId::new(),
		};
		Ok(arrow_function)
	}

	pub(crate) fn from_reader_sub_open_paren(
		reader: &mut impl TokenReader<TSXToken, Span>,
		state: &mut crate::ParsingState,
		settings: &ParseSettings,
		is_async: Option<Keyword<tsx_keywords::Async>>,
		open_paren_span: Span,
	) -> ParseResult<Self> {
		let parameters = FunctionParameters::from_reader_sub_open_parenthesis(
			reader,
			state,
			settings,
			open_paren_span,
		)?;
		let return_type = if matches!(reader.peek().unwrap().0, TSXToken::Colon) {
			reader.next();
			Some(TypeReference::from_reader(reader, state, settings)?)
		} else {
			None
		};
		reader.expect_next(TSXToken::Arrow)?;
		let body = ExpressionOrBlock::from_reader(reader, state, settings)?;
		Ok(FunctionBase {
			header: is_async,
			name: (),
			parameters,
			return_type,
			type_parameters: None,
			body,
			function_id: FunctionId::new(),
		})
	}
}

/// For [ArrowFunction] and [crate::MatchArm] bodies
#[derive(Debug, Clone, Eq, PartialEq, Visitable)]
// #[cfg_attr(feature = "self-rust-tokenize", derive(self_rust_tokenize::SelfRustTokenize))]
pub enum ExpressionOrBlock {
	Expression(Box<Expression>),
	Block(Block),
}

impl ExpressionOrBlock {
	pub fn get_block_id(&self) -> Option<BlockId> {
		match self {
			ExpressionOrBlock::Expression(_) => None,
			ExpressionOrBlock::Block(block) => Some(block.1),
		}
	}
}

impl ASTNode for ExpressionOrBlock {
	fn get_position(&self) -> Cow<Span> {
		match self {
			ExpressionOrBlock::Expression(expression) => expression.get_position(),
			ExpressionOrBlock::Block(block) => block.get_position(),
		}
	}

	fn from_reader(
		reader: &mut impl TokenReader<TSXToken, Span>,
		state: &mut crate::ParsingState,
		settings: &ParseSettings,
	) -> ParseResult<Self> {
		if matches!(reader.peek().unwrap().0, TSXToken::OpenBrace) {
			Ok(Self::Block(Block::from_reader(reader, state, settings)?))
		} else {
			let expression = Expression::from_reader(reader, state, settings)?;
			Ok(Self::Expression(Box::new(expression)))
		}
	}

	fn to_string_from_buffer<T: source_map::ToString>(
		&self,
		buf: &mut T,
		settings: &crate::ToStringSettings,
		depth: u8,
	) {
		match self {
			ExpressionOrBlock::Expression(expr) => expr.to_string_from_buffer(buf, settings, depth),
			ExpressionOrBlock::Block(block) => block.to_string_from_buffer(buf, settings, depth),
		}
	}
}
