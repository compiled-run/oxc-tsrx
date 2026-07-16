use crate::model::{
    ByteSpan, Clause, ClauseRole, ControlContext, ControlKind, DynamicTag, EmbeddedKind,
    EmbeddedToken, ForHeader, NONE, Overlay, ProjectionError, StructuralKind, StructuralToken,
    StyleBlock, SyntaxNode, to_u32,
};

#[derive(Clone, Copy)]
struct Checkpoint {
    tokens: usize,
    nodes: usize,
    clauses: usize,
    embedded_tokens: usize,
    dynamic_tags: usize,
    dynamic_comments: usize,
    style_blocks: usize,
    first_root: u32,
    last_root: u32,
    parent: Option<(usize, u32, u32)>,
}

struct TinyStack<T: Copy, const N: usize> {
    inline: [Option<T>; N],
    length: usize,
    spill: Vec<T>,
}

impl<T: Copy, const N: usize> TinyStack<T, N> {
    fn new() -> Self {
        Self {
            inline: [None; N],
            length: 0,
            spill: Vec::new(),
        }
    }

    fn push(&mut self, value: T) {
        if self.length < N {
            self.inline[self.length] = Some(value);
        } else {
            self.spill.push(value);
        }
        self.length += 1;
    }

    fn pop(&mut self) -> Option<T> {
        if self.length == 0 {
            return None;
        }
        self.length -= 1;
        if self.length < N {
            let value = self.inline[self.length];
            self.inline[self.length] = None;
            value
        } else {
            self.spill.pop()
        }
    }

    fn last(&self) -> Option<T> {
        if self.length == 0 {
            None
        } else if self.length <= N {
            self.inline[self.length - 1]
        } else {
            self.spill.last().copied()
        }
    }

    const fn is_empty(&self) -> bool {
        self.length == 0
    }
}

pub(crate) struct Scanner<'a> {
    bytes: &'a [u8],
    tokens: Vec<StructuralToken>,
    nodes: Vec<SyntaxNode>,
    clauses: Vec<Clause>,
    embedded_tokens: Vec<EmbeddedToken>,
    dynamic_tags: Vec<DynamicTag>,
    dynamic_comments: Vec<ByteSpan>,
    style_blocks: Vec<StyleBlock>,
    first_root: u32,
    last_root: u32,
    parents: Vec<u32>,
}

impl<'a> Scanner<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        let bytes = source.as_bytes();
        Self {
            bytes,
            tokens: Vec::with_capacity(bytes.len().div_ceil(384)),
            nodes: Vec::with_capacity(bytes.len().div_ceil(1024)),
            clauses: Vec::with_capacity(bytes.len().div_ceil(512)),
            // Dynamic tags and raw styles are sparse. Keep the common zero-syntax path free of
            // avoidable heap allocations; the flat vectors grow after the first commit.
            embedded_tokens: Vec::new(),
            dynamic_tags: Vec::new(),
            dynamic_comments: Vec::new(),
            style_blocks: Vec::new(),
            first_root: NONE,
            last_root: NONE,
            parents: Vec::with_capacity(8),
        }
    }

    pub(crate) fn finish(mut self) -> Result<Overlay, ProjectionError> {
        let source_len = to_u32(self.bytes.len())?;
        self.scan_region(0, None)?;
        Ok(Overlay {
            source_len,
            source_fingerprint: source_fingerprint(self.bytes),
            tokens: self.tokens,
            nodes: self.nodes,
            clauses: self.clauses,
            embedded_tokens: self.embedded_tokens,
            dynamic_tags: self.dynamic_tags,
            dynamic_comments: self.dynamic_comments,
            style_blocks: self.style_blocks,
            first_root: self.first_root,
            last_root: self.last_root,
        })
    }

    #[allow(clippy::too_many_lines)]
    fn scan_region(
        &mut self,
        mut index: usize,
        closing: Option<u8>,
    ) -> Result<usize, ProjectionError> {
        let mut delimiters = TinyStack::<(u8, bool), 16>::new();
        if let Some(closing) = closing {
            delimiters.push((closing, closing == b'}'));
        }
        let mut can_start_expression = true;
        let mut can_start_jsx = true;
        let mut pending_control_paren = false;
        let mut closed_control_paren = false;
        let mut parens = TinyStack::<bool, 16>::new();

        while index < self.bytes.len() {
            let byte = self.bytes[index];
            if byte.is_ascii_whitespace() {
                index += 1;
                continue;
            }

            match byte {
                b'\'' | b'"' => {
                    index = self.skip_quote(index, byte)?;
                    can_start_expression = false;
                    can_start_jsx = false;
                    pending_control_paren = false;
                    closed_control_paren = false;
                }
                b'`' => {
                    index = self.scan_template(index)?;
                    can_start_expression = false;
                    can_start_jsx = false;
                    pending_control_paren = false;
                    closed_control_paren = false;
                }
                b'/' if self.bytes.get(index + 1) == Some(&b'/') => {
                    index = self.skip_line_comment(index + 2);
                }
                b'/' if self.bytes.get(index + 1) == Some(&b'*') => {
                    index = self.skip_block_comment(index)?;
                }
                b'/' if can_start_expression => {
                    index = self.skip_regex(index)?;
                    can_start_expression = false;
                    can_start_jsx = false;
                    pending_control_paren = false;
                    closed_control_paren = false;
                }
                b'/' => {
                    index += usize::from(self.bytes.get(index + 1) == Some(&b'=')) + 1;
                    can_start_expression = true;
                    can_start_jsx = true;
                    pending_control_paren = false;
                    closed_control_paren = false;
                }
                b'<' if can_start_jsx && self.looks_like_jsx_start(index) => {
                    let checkpoint = self.checkpoint();
                    let committed = self.committed_jsx_opening(index);
                    match self.scan_jsx_element(index) {
                        Ok(end) => {
                            index = end;
                            can_start_expression = false;
                            can_start_jsx = true;
                            pending_control_paren = false;
                            closed_control_paren = false;
                        }
                        Err(ProjectionError::UnsupportedSyntax { offset, construct }) => {
                            return Err(ProjectionError::UnsupportedSyntax { offset, construct });
                        }
                        Err(error) if committed => return Err(error),
                        Err(_) => {
                            self.rollback(checkpoint);
                            index += 1;
                            can_start_expression = true;
                            can_start_jsx = false;
                            pending_control_paren = false;
                            closed_control_paren = false;
                        }
                    }
                }
                b'@' if self.keyword_at(index, b"if") => {
                    index = self.parse_if(index, self.code_context(index))?;
                    can_start_expression = false;
                    can_start_jsx = true;
                    pending_control_paren = false;
                    closed_control_paren = false;
                }
                b'@' if self.keyword_at(index, b"for") => {
                    index = self.parse_for(index, self.code_context(index))?;
                    can_start_expression = false;
                    can_start_jsx = true;
                    pending_control_paren = false;
                    closed_control_paren = false;
                }
                b'@' if self.keyword_at(index, b"switch") => {
                    index = self.parse_switch(index, self.code_context(index))?;
                    can_start_expression = false;
                    can_start_jsx = true;
                    pending_control_paren = false;
                    closed_control_paren = false;
                }
                b'@' if self.keyword_at(index, b"try") => {
                    index = self.parse_try(index, self.code_context(index))?;
                    can_start_expression = false;
                    can_start_jsx = true;
                    pending_control_paren = false;
                    closed_control_paren = false;
                }
                b'@' if self.bytes.get(index + 1) == Some(&b'{') => {
                    self.push_token(StructuralKind::FunctionBody, index)?;
                    index += 1;
                    can_start_expression = true;
                    can_start_jsx = true;
                    pending_control_paren = false;
                    closed_control_paren = false;
                }
                b'@' => {
                    if self.keyword_at(index, b"else")
                        || self.keyword_at(index, b"empty")
                        || self.keyword_at(index, b"case")
                        || self.keyword_at(index, b"default")
                        || self.keyword_at(index, b"pending")
                        || self.keyword_at(index, b"catch")
                    {
                        return Err(ProjectionError::MalformedSyntax {
                            offset: to_u32(index)?,
                            expected: "an owning TSRX control",
                        });
                    }
                    if let Some(construct) = unsupported_at_construct(self.bytes, index) {
                        return Err(ProjectionError::UnsupportedSyntax {
                            offset: to_u32(index)?,
                            construct,
                        });
                    }
                    index += 1;
                    can_start_expression = true;
                    can_start_jsx = true;
                    pending_control_paren = false;
                    closed_control_paren = false;
                }
                b'(' | b'[' | b'{' => {
                    let close = match byte {
                        b'(' => b')',
                        b'[' => b']',
                        b'{' => b'}',
                        _ => unreachable!(),
                    };
                    let previous = previous_significant_byte(self.bytes, index);
                    let block = byte == b'{'
                        && (!can_start_expression
                            || closed_control_paren
                            || previous == Some(b'@')
                            || previous == Some(b'>')
                                && previous_significant_byte(self.bytes, index.saturating_sub(1))
                                    == Some(b'='));
                    delimiters.push((close, block));
                    if byte == b'(' {
                        parens.push(pending_control_paren);
                    }
                    pending_control_paren = false;
                    closed_control_paren = false;
                    index += 1;
                    can_start_expression = true;
                    can_start_jsx = true;
                }
                b')' | b']' | b'}' => {
                    let mut closed_block = false;
                    if delimiters
                        .last()
                        .is_some_and(|delimiter| delimiter.0 == byte)
                    {
                        closed_block = delimiters.pop().is_some_and(|delimiter| delimiter.1);
                        index += 1;
                        if delimiters.is_empty() && closing.is_some() {
                            return Ok(index);
                        }
                    } else if closing.is_some() {
                        return Err(ProjectionError::MalformedSyntax {
                            offset: to_u32(index)?,
                            expected: "a matching delimiter",
                        });
                    } else {
                        index += 1;
                    }
                    can_start_expression = if byte == b')' {
                        let control = parens.pop().unwrap_or(false);
                        closed_control_paren = control;
                        control
                    } else if byte == b'}' {
                        closed_control_paren = false;
                        closed_block
                    } else {
                        closed_control_paren = false;
                        false
                    };
                    can_start_jsx = (byte == b'}' && closed_block) || can_start_expression;
                    pending_control_paren = false;
                }
                b'0'..=b'9' => {
                    index = self.skip_number(index);
                    can_start_expression = false;
                    can_start_jsx = false;
                    pending_control_paren = false;
                    closed_control_paren = false;
                }
                byte if is_identifier_start(byte) => {
                    let end = self.skip_identifier(index);
                    let identifier = &self.bytes[index..end];
                    pending_control_paren = matches!(
                        identifier,
                        b"if" | b"for" | b"while" | b"with" | b"switch" | b"catch"
                    );
                    can_start_expression = pending_control_paren
                        || matches!(
                            identifier,
                            b"return"
                                | b"throw"
                                | b"case"
                                | b"delete"
                                | b"void"
                                | b"typeof"
                                | b"new"
                                | b"yield"
                                | b"await"
                                | b"in"
                                | b"of"
                                | b"instanceof"
                                | b"else"
                                | b"do"
                        );
                    can_start_jsx = can_start_expression;
                    closed_control_paren = false;
                    index = end;
                }
                b'+' | b'-'
                    if self.bytes.get(index + 1) == Some(&byte) && !can_start_expression =>
                {
                    index += 2;
                    can_start_expression = false;
                    can_start_jsx = false;
                    pending_control_paren = false;
                    closed_control_paren = false;
                }
                b'.' => {
                    index += if self.bytes.get(index..index + 3) == Some(b"...") {
                        3
                    } else {
                        1
                    };
                    can_start_expression = false;
                    can_start_jsx = false;
                    pending_control_paren = false;
                    closed_control_paren = false;
                }
                _ => {
                    index += 1;
                    can_start_expression = !matches!(byte, b']');
                    can_start_jsx = can_start_expression || matches!(byte, b';');
                    pending_control_paren = false;
                    closed_control_paren = false;
                }
            }
        }

        if closing.is_some() {
            return Err(ProjectionError::UnterminatedSyntax {
                offset: to_u32(index.saturating_sub(1))?,
                construct: "delimited expression",
            });
        }
        Ok(index)
    }

    fn parse_if(
        &mut self,
        start: usize,
        context: ControlContext,
    ) -> Result<usize, ProjectionError> {
        let node = self.begin_node(ControlKind::If, context, start)?;
        self.parents.push(node);
        let result = self.parse_if_clauses(node, start);
        self.parents.pop();
        let end = result?;
        self.nodes[node as usize].span.end = to_u32(end)?;
        Ok(end)
    }

    fn parse_if_clauses(&mut self, node: u32, start: usize) -> Result<usize, ProjectionError> {
        self.push_token(StructuralKind::If, start)?;
        let mut index = Self::after_keyword(start, b"if");
        index = self.skip_trivia(index)?;
        let (header, after_header) = self.parse_parenthesized(index)?;
        index = self.skip_trivia(after_header)?;
        let body = self.parse_body(node, index)?;
        self.add_clause(
            node,
            ClauseRole::If,
            start,
            header,
            body,
            ForHeader::default(),
        )?;
        index = body.end as usize;

        loop {
            let clause_start = self.skip_trivia(index)?;
            if self.keyword_at(clause_start, b"else") {
                self.push_token(StructuralKind::Else, clause_start)?;
                let mut after_else = Self::after_keyword(clause_start, b"else");
                after_else = self.skip_trivia(after_else)?;
                if self.bare_keyword_at(after_else, b"if") {
                    let keyword_end = Self::after_bare_keyword(after_else, b"if");
                    let header_start = self.skip_trivia(keyword_end)?;
                    let (header, after_header) = self.parse_parenthesized(header_start)?;
                    let body_start = self.skip_trivia(after_header)?;
                    let body = self.parse_body(node, body_start)?;
                    self.add_clause(
                        node,
                        ClauseRole::ElseIf,
                        clause_start,
                        header,
                        body,
                        ForHeader::default(),
                    )?;
                    index = body.end as usize;
                    continue;
                }
                let body = self.parse_body(node, after_else)?;
                self.add_clause(
                    node,
                    ClauseRole::Else,
                    clause_start,
                    ByteSpan::default(),
                    body,
                    ForHeader::default(),
                )?;
                return Ok(body.end as usize);
            }
            if self.bare_keyword_at(clause_start, b"else") {
                return Err(ProjectionError::MalformedSyntax {
                    offset: to_u32(clause_start)?,
                    expected: "`@else`",
                });
            }
            return Ok(index);
        }
    }

    fn parse_for(
        &mut self,
        start: usize,
        context: ControlContext,
    ) -> Result<usize, ProjectionError> {
        let node = self.begin_node(ControlKind::For, context, start)?;
        self.parents.push(node);
        let result = self.parse_for_parts(node, start);
        self.parents.pop();
        let end = result?;
        self.nodes[node as usize].span.end = to_u32(end)?;
        Ok(end)
    }

    fn parse_for_parts(&mut self, node: u32, start: usize) -> Result<usize, ProjectionError> {
        self.push_token(StructuralKind::For, start)?;
        let mut index = Self::after_keyword(start, b"for");
        index = self.skip_trivia(index)?;
        let mut is_await = false;
        if self.bare_keyword_at(index, b"await") {
            is_await = true;
            index = self.skip_trivia(Self::after_bare_keyword(index, b"await"))?;
        }
        let (header, after_header) = self.parse_parenthesized(index)?;
        let mut for_header = self.analyze_for_header(header)?;
        for_header.r#await = is_await;
        index = self.skip_trivia(after_header)?;
        let body = self.parse_body(node, index)?;
        self.add_clause(node, ClauseRole::For, start, header, body, for_header)?;
        index = body.end as usize;

        let clause_start = self.skip_trivia(index)?;
        if self.keyword_at(clause_start, b"empty") {
            self.push_token(StructuralKind::Empty, clause_start)?;
            let body_start = self.skip_trivia(Self::after_keyword(clause_start, b"empty"))?;
            let empty_body = self.parse_body(node, body_start)?;
            self.add_clause(
                node,
                ClauseRole::Empty,
                clause_start,
                ByteSpan::default(),
                empty_body,
                ForHeader::default(),
            )?;
            return Ok(empty_body.end as usize);
        }
        if self.bare_keyword_at(clause_start, b"empty") {
            return Err(ProjectionError::MalformedSyntax {
                offset: to_u32(clause_start)?,
                expected: "`@empty`",
            });
        }
        Ok(index)
    }

    fn parse_switch(
        &mut self,
        start: usize,
        context: ControlContext,
    ) -> Result<usize, ProjectionError> {
        let node = self.begin_node(ControlKind::Switch, context, start)?;
        self.parents.push(node);
        let result = self.parse_switch_parts(node, start);
        self.parents.pop();
        let end = result?;
        self.nodes[node as usize].span.end = to_u32(end)?;
        Ok(end)
    }

    fn parse_switch_parts(&mut self, node: u32, start: usize) -> Result<usize, ProjectionError> {
        self.push_token(StructuralKind::Switch, start)?;
        let header_start = self.skip_trivia(Self::after_keyword(start, b"switch"))?;
        let (_, after_header) = self.parse_parenthesized(header_start)?;
        let mut index = self.skip_trivia(after_header)?;
        if self.bytes.get(index) != Some(&b'{') {
            return Err(ProjectionError::MalformedSyntax {
                offset: to_u32(index)?,
                expected: "a braced `@switch` body",
            });
        }
        index += 1;
        let mut saw_default = false;
        loop {
            index = self.skip_trivia(index)?;
            if self.bytes.get(index) == Some(&b'}') {
                return Ok(index + 1);
            }
            if self.keyword_at(index, b"case") {
                self.push_token(StructuralKind::Case, index)?;
                let value_start = self.skip_trivia(Self::after_keyword(index, b"case"))?;
                let (header, colon) = self.parse_case_header(value_start)?;
                let body_start = self.skip_trivia(colon + 1)?;
                let body = self.parse_body(node, body_start)?;
                self.add_clause(
                    node,
                    ClauseRole::Case,
                    index,
                    header,
                    body,
                    ForHeader::default(),
                )?;
                index = body.end as usize;
                continue;
            }
            if self.keyword_at(index, b"default") {
                if saw_default {
                    return Err(ProjectionError::MalformedSyntax {
                        offset: to_u32(index)?,
                        expected: "only one `@default` clause",
                    });
                }
                saw_default = true;
                self.push_token(StructuralKind::Default, index)?;
                let colon = self.skip_trivia(Self::after_keyword(index, b"default"))?;
                if self.bytes.get(colon) != Some(&b':') {
                    return Err(ProjectionError::MalformedSyntax {
                        offset: to_u32(colon)?,
                        expected: "`:` after `@default`",
                    });
                }
                let body_start = self.skip_trivia(colon + 1)?;
                let body = self.parse_body(node, body_start)?;
                self.add_clause(
                    node,
                    ClauseRole::Default,
                    index,
                    ByteSpan::default(),
                    body,
                    ForHeader::default(),
                )?;
                index = body.end as usize;
                continue;
            }
            if self.bare_keyword_at(index, b"case") || self.bare_keyword_at(index, b"default") {
                return Err(ProjectionError::MalformedSyntax {
                    offset: to_u32(index)?,
                    expected: "an `@case` or `@default` clause",
                });
            }
            return Err(ProjectionError::MalformedSyntax {
                offset: to_u32(index)?,
                expected: "an `@case`, `@default`, or closing `}`",
            });
        }
    }

    fn parse_try(
        &mut self,
        start: usize,
        context: ControlContext,
    ) -> Result<usize, ProjectionError> {
        let node = self.begin_node(ControlKind::Try, context, start)?;
        self.parents.push(node);
        let result = self.parse_try_parts(node, start);
        self.parents.pop();
        let end = result?;
        self.nodes[node as usize].span.end = to_u32(end)?;
        Ok(end)
    }

    fn parse_try_parts(&mut self, node: u32, start: usize) -> Result<usize, ProjectionError> {
        self.push_token(StructuralKind::Try, start)?;
        let body_start = self.skip_trivia(Self::after_keyword(start, b"try"))?;
        let body = self.parse_body(node, body_start)?;
        self.add_clause(
            node,
            ClauseRole::Try,
            start,
            ByteSpan::default(),
            body,
            ForHeader::default(),
        )?;
        let mut index = body.end as usize;
        let mut has_pending = false;
        let mut has_catch = false;

        let pending_start = self.skip_trivia(index)?;
        if self.keyword_at(pending_start, b"pending") {
            has_pending = true;
            self.push_token(StructuralKind::Pending, pending_start)?;
            let pending_body_start =
                self.skip_trivia(Self::after_keyword(pending_start, b"pending"))?;
            let pending_body = self.parse_body(node, pending_body_start)?;
            self.add_clause(
                node,
                ClauseRole::Pending,
                pending_start,
                ByteSpan::default(),
                pending_body,
                ForHeader::default(),
            )?;
            index = pending_body.end as usize;
        } else if self.bare_keyword_at(pending_start, b"pending") {
            return Err(ProjectionError::MalformedSyntax {
                offset: to_u32(pending_start)?,
                expected: "`@pending`",
            });
        }

        let catch_start = self.skip_trivia(index)?;
        if self.keyword_at(catch_start, b"catch") {
            has_catch = true;
            self.push_token(StructuralKind::Catch, catch_start)?;
            let after_keyword = self.skip_trivia(Self::after_keyword(catch_start, b"catch"))?;
            let (header, bindings, catch_body_start) =
                if self.bytes.get(after_keyword) == Some(&b'(') {
                    let (header, after_header) = self.parse_parenthesized(after_keyword)?;
                    let bindings = self.catch_binding_count(header)?;
                    (header, bindings, self.skip_trivia(after_header)?)
                } else {
                    (ByteSpan::default(), 0, after_keyword)
                };
            let catch_body = self.parse_body(node, catch_body_start)?;
            self.add_clause_with_bindings(
                node,
                ClauseRole::Catch,
                catch_start,
                header,
                catch_body,
                ForHeader::default(),
                bindings,
            )?;
            index = catch_body.end as usize;
        } else if self.bare_keyword_at(catch_start, b"catch") {
            return Err(ProjectionError::MalformedSyntax {
                offset: to_u32(catch_start)?,
                expected: "`@catch`",
            });
        }

        if !has_pending && !has_catch {
            return Err(ProjectionError::MalformedSyntax {
                offset: to_u32(start)?,
                expected: "an `@pending` or `@catch` clause",
            });
        }
        let trailing = self.skip_trivia(index)?;
        if self.keyword_at(trailing, b"pending") || self.bare_keyword_at(trailing, b"pending") {
            return Err(ProjectionError::MalformedSyntax {
                offset: to_u32(trailing)?,
                expected: "at most one `@pending` before `@catch`",
            });
        }
        if self.keyword_at(trailing, b"catch") || self.bare_keyword_at(trailing, b"catch") {
            return Err(ProjectionError::MalformedSyntax {
                offset: to_u32(trailing)?,
                expected: "at most one `@catch` clause",
            });
        }
        Ok(index)
    }

    fn parse_case_header(&self, start: usize) -> Result<(ByteSpan, usize), ProjectionError> {
        let mut index = start;
        let mut delimiters = TinyStack::<u8, 16>::new();
        let mut can_start_expression = true;
        let mut ternaries = 0usize;
        while index < self.bytes.len() {
            let byte = self.bytes[index];
            match byte {
                b'\'' | b'"' => {
                    index = self.skip_quote(index, byte)?;
                    can_start_expression = false;
                }
                b'`' => {
                    index = self.skip_template_raw(index, self.bytes.len())?;
                    can_start_expression = false;
                }
                b'/' if self.bytes.get(index + 1) == Some(&b'/') => {
                    index = self.skip_line_comment(index + 2);
                }
                b'/' if self.bytes.get(index + 1) == Some(&b'*') => {
                    index = self.skip_block_comment(index)?;
                }
                b'/' if can_start_expression => {
                    index = self.skip_regex(index)?;
                    can_start_expression = false;
                }
                b'(' | b'[' | b'{' => {
                    delimiters.push(match byte {
                        b'(' => b')',
                        b'[' => b']',
                        _ => b'}',
                    });
                    index += 1;
                    can_start_expression = true;
                }
                b')' | b']' | b'}' => {
                    if delimiters.last() == Some(byte) {
                        delimiters.pop();
                        index += 1;
                        can_start_expression = false;
                    } else {
                        break;
                    }
                }
                b'?' if delimiters.is_empty()
                    && self.bytes.get(index + 1) != Some(&b'.')
                    && self.bytes.get(index + 1) != Some(&b'?') =>
                {
                    ternaries = ternaries.saturating_add(1);
                    index += 1;
                    can_start_expression = true;
                }
                b':' if delimiters.is_empty() && ternaries > 0 => {
                    ternaries -= 1;
                    index += 1;
                    can_start_expression = true;
                }
                b':' if delimiters.is_empty() => {
                    let end = trim_ascii_end(self.bytes, start, index);
                    if end == start {
                        return Err(ProjectionError::MalformedSyntax {
                            offset: to_u32(start)?,
                            expected: "a case expression before `:`",
                        });
                    }
                    return Ok((ByteSpan::new(to_u32(start)?, to_u32(end)?), index));
                }
                byte if is_identifier_start(byte) => {
                    index = self.skip_identifier(index);
                    can_start_expression = false;
                }
                byte if byte.is_ascii_whitespace() => index += 1,
                _ => {
                    can_start_expression = matches!(
                        byte,
                        b'=' | b',' | b':' | b'?' | b'!' | b'+' | b'-' | b'*' | b'%' | b'&' | b'|'
                    );
                    index += 1;
                }
            }
        }
        Err(ProjectionError::MalformedSyntax {
            offset: to_u32(index.min(self.bytes.len()))?,
            expected: "`:` after an `@case` expression",
        })
    }

    fn catch_binding_count(&self, header: ByteSpan) -> Result<u8, ProjectionError> {
        let inner_start = header.start as usize + 1;
        let inner_end = header.end as usize - 1;
        let commas = self.top_level_separators(inner_start, inner_end, b',')?;
        if commas.len() > 1 {
            return Err(ProjectionError::MalformedSyntax {
                offset: to_u32(commas[1])?,
                expected: "at most error and reset bindings in `@catch`",
            });
        }
        let first_start = self.skip_ascii_whitespace(inner_start, inner_end);
        let first_end = trim_ascii_end(
            self.bytes,
            first_start,
            commas.first().copied().unwrap_or(inner_end),
        );
        if first_start == first_end {
            return Err(ProjectionError::MalformedSyntax {
                offset: to_u32(first_start)?,
                expected: "an error binding in `@catch (...)`",
            });
        }
        let Some(&comma) = commas.first() else {
            return Ok(1);
        };
        let reset_start = self.skip_ascii_whitespace(comma + 1, inner_end);
        let reset_end = trim_ascii_end(self.bytes, reset_start, inner_end);
        if reset_start == reset_end || !is_identifier_start(self.bytes[reset_start]) {
            return Err(ProjectionError::MalformedSyntax {
                offset: to_u32(reset_start)?,
                expected: "a reset identifier after the catch error binding",
            });
        }
        let identifier_end = self.skip_identifier(reset_start);
        let remainder = self.skip_ascii_whitespace(identifier_end, reset_end);
        if remainder < reset_end
            && (self.bytes[remainder] != b':'
                || self.skip_ascii_whitespace(remainder + 1, reset_end) == reset_end)
        {
            return Err(ProjectionError::MalformedSyntax {
                offset: to_u32(remainder)?,
                expected: "a reset identifier with an optional type annotation",
            });
        }
        Ok(2)
    }

    fn parse_parenthesized(&mut self, start: usize) -> Result<(ByteSpan, usize), ProjectionError> {
        if self.bytes.get(start) != Some(&b'(') {
            return Err(ProjectionError::MalformedSyntax {
                offset: to_u32(start)?,
                expected: "`(`",
            });
        }
        let end = self.scan_region(start + 1, Some(b')'))?;
        Ok((ByteSpan::new(to_u32(start)?, to_u32(end)?), end))
    }

    fn parse_body(&mut self, node: u32, start: usize) -> Result<ByteSpan, ProjectionError> {
        if self.bytes.get(start) != Some(&b'{') {
            return Err(ProjectionError::MalformedSyntax {
                offset: to_u32(start)?,
                expected: "a braced control-flow body",
            });
        }
        debug_assert_eq!(self.parents.last().copied(), Some(node));
        let end = self.scan_region(start + 1, Some(b'}'))?;
        Ok(ByteSpan::new(to_u32(start)?, to_u32(end)?))
    }

    fn analyze_for_header(&self, header: ByteSpan) -> Result<ForHeader, ProjectionError> {
        let inner_start = header.start as usize + 1;
        let inner_end = header.end as usize - 1;
        let semicolons = self.top_level_separators(inner_start, inner_end, b';')?;
        let Some(&first) = semicolons.first() else {
            return Ok(ForHeader::default());
        };
        let Some(of) = self.find_top_level_keyword(inner_start, first, b"of")? else {
            return Ok(ForHeader::default());
        };
        let first_value = self.skip_ascii_whitespace(first + 1, inner_end);
        if !self.bare_keyword_at(first_value, b"index")
            && !self.bare_keyword_at(first_value, b"key")
        {
            return Ok(ForHeader::default());
        }

        let base_end = trim_ascii_end(self.bytes, inner_start, first);
        let left_end = trim_ascii_end(self.bytes, inner_start, of);
        let right_start = self.skip_ascii_whitespace(of + 2, base_end);
        let right_end = trim_ascii_end(self.bytes, right_start, base_end);
        if left_end <= inner_start || right_end <= right_start {
            return Err(ProjectionError::MalformedSyntax {
                offset: to_u32(of)?,
                expected: "a complete `for ... of ...` header",
            });
        }

        let mut result = ForHeader {
            left: ByteSpan::new(to_u32(inner_start)?, to_u32(left_end)?),
            right: ByteSpan::new(to_u32(right_start)?, to_u32(right_end)?),
            annotated: true,
            ..ForHeader::default()
        };
        for (position, &semi) in semicolons.iter().enumerate() {
            let segment_end = semicolons.get(position + 1).copied().unwrap_or(inner_end);
            let keyword_start = self.skip_ascii_whitespace(semi + 1, segment_end);
            let (kind, keyword_len) = if self.bare_keyword_at(keyword_start, b"index") {
                (ClauseRole::For, 5)
            } else if self.bare_keyword_at(keyword_start, b"key") {
                (ClauseRole::Empty, 3)
            } else {
                return Err(ProjectionError::MalformedSyntax {
                    offset: to_u32(keyword_start)?,
                    expected: "`index` or `key` annotation",
                });
            };
            let value_start = self.skip_ascii_whitespace(keyword_start + keyword_len, segment_end);
            let value_end = trim_ascii_end(self.bytes, value_start, segment_end);
            if value_start == value_end {
                return Err(ProjectionError::MalformedSyntax {
                    offset: to_u32(value_start)?,
                    expected: "an annotation value",
                });
            }
            let span = ByteSpan::new(to_u32(value_start)?, to_u32(value_end)?);
            if matches!(kind, ClauseRole::For)
                && (!is_identifier_start(self.bytes[value_start])
                    || self.skip_identifier(value_start) != value_end)
            {
                return Err(ProjectionError::MalformedSyntax {
                    offset: to_u32(value_start)?,
                    expected: "an identifier after `index`",
                });
            }
            match kind {
                ClauseRole::For if result.index.is_empty() && result.key.is_empty() => {
                    result.index = span;
                }
                ClauseRole::Empty if result.key.is_empty() => result.key = span,
                ClauseRole::For => {
                    return Err(ProjectionError::MalformedSyntax {
                        offset: to_u32(keyword_start)?,
                        expected: "one `index` annotation before `key`",
                    });
                }
                ClauseRole::Empty => {
                    return Err(ProjectionError::MalformedSyntax {
                        offset: to_u32(keyword_start)?,
                        expected: "one `key` annotation",
                    });
                }
                _ => unreachable!(),
            }
        }
        Ok(result)
    }

    fn top_level_separators(
        &self,
        mut index: usize,
        end: usize,
        separator: u8,
    ) -> Result<Vec<usize>, ProjectionError> {
        let mut delimiters = TinyStack::<u8, 16>::new();
        let mut result = Vec::new();
        let mut can_start_expression = true;
        while index < end {
            match self.bytes[index] {
                b'\'' | b'"' => {
                    index = self.skip_quote(index, self.bytes[index])?;
                    can_start_expression = false;
                }
                b'`' => index = self.skip_template_raw(index, end)?,
                b'/' if self.bytes.get(index + 1) == Some(&b'/') => {
                    index = self.skip_line_comment(index + 2);
                }
                b'/' if self.bytes.get(index + 1) == Some(&b'*') => {
                    index = self.skip_block_comment(index)?;
                }
                b'/' if can_start_expression => {
                    index = self.skip_regex(index)?;
                    can_start_expression = false;
                }
                b'(' | b'[' | b'{' => {
                    delimiters.push(self.bytes[index]);
                    index += 1;
                    can_start_expression = true;
                }
                b')' | b']' | b'}' => {
                    delimiters.pop();
                    index += 1;
                    can_start_expression = false;
                }
                byte if byte == separator && delimiters.is_empty() => {
                    result.push(index);
                    index += 1;
                    can_start_expression = true;
                }
                byte if is_identifier_start(byte) => {
                    index = self.skip_identifier(index);
                    can_start_expression = false;
                }
                _ => {
                    can_start_expression = matches!(
                        self.bytes[index],
                        b'=' | b',' | b':' | b'?' | b'!' | b'+' | b'-' | b'*' | b'%' | b'&' | b'|'
                    );
                    index += 1;
                }
            }
        }
        Ok(result)
    }

    fn find_top_level_keyword(
        &self,
        mut index: usize,
        end: usize,
        keyword: &[u8],
    ) -> Result<Option<usize>, ProjectionError> {
        let mut delimiters = TinyStack::<u8, 16>::new();
        while index < end {
            match self.bytes[index] {
                b'\'' | b'"' => index = self.skip_quote(index, self.bytes[index])?,
                b'`' => index = self.skip_template_raw(index, end)?,
                b'/' if self.bytes.get(index + 1) == Some(&b'/') => {
                    index = self.skip_line_comment(index + 2);
                }
                b'/' if self.bytes.get(index + 1) == Some(&b'*') => {
                    index = self.skip_block_comment(index)?;
                }
                b'(' | b'[' | b'{' => {
                    delimiters.push(self.bytes[index]);
                    index += 1;
                }
                b')' | b']' | b'}' => {
                    delimiters.pop();
                    index += 1;
                }
                byte if delimiters.is_empty() && is_identifier_start(byte) => {
                    let word_end = self.skip_identifier(index);
                    if &self.bytes[index..word_end] == keyword {
                        return Ok(Some(index));
                    }
                    index = word_end;
                }
                _ => index += 1,
            }
        }
        Ok(None)
    }

    fn scan_template(&mut self, start: usize) -> Result<usize, ProjectionError> {
        let mut index = start + 1;
        let mut escaped = false;
        while index < self.bytes.len() {
            let byte = self.bytes[index];
            if escaped {
                escaped = false;
                index += 1;
            } else if byte == b'\\' {
                escaped = true;
                index += 1;
            } else if byte == b'`' {
                return Ok(index + 1);
            } else if byte == b'$' && self.bytes.get(index + 1) == Some(&b'{') {
                index = self.scan_region(index + 2, Some(b'}'))?;
            } else {
                index += 1;
            }
        }
        Err(ProjectionError::UnterminatedSyntax {
            offset: to_u32(start)?,
            construct: "template literal",
        })
    }

    fn skip_template_raw(&self, start: usize, end: usize) -> Result<usize, ProjectionError> {
        let mut index = start + 1;
        let mut escaped = false;
        let mut braces = 0usize;
        while index < end {
            let byte = self.bytes[index];
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'`' && braces == 0 {
                return Ok(index + 1);
            } else if byte == b'$' && self.bytes.get(index + 1) == Some(&b'{') {
                braces += 1;
                index += 1;
            } else if byte == b'}' && braces > 0 {
                braces -= 1;
            }
            index += 1;
        }
        Err(ProjectionError::UnterminatedSyntax {
            offset: to_u32(start)?,
            construct: "template literal",
        })
    }

    #[allow(clippy::too_many_lines)]
    fn scan_jsx_element(&mut self, start: usize) -> Result<usize, ProjectionError> {
        let mut index = start + 1;
        let fragment = self.bytes.get(index) == Some(&b'>');
        let dynamic = self.bytes.get(index) == Some(&b'{');
        let name_start = index;
        let name_end;
        let mut dynamic_identity = ByteSpan::default();
        let mut dynamic_owner = None;
        if fragment {
            name_end = name_start;
            index += 1;
        } else if dynamic {
            let (expression, end) = self.scan_dynamic_expression(index)?;
            let identity = self.validate_dynamic_expression(expression)?;
            dynamic_identity = identity;
            name_end = name_start;
            index = end;
            let owner = to_u32(self.dynamic_tags.len())?;
            self.dynamic_tags.push(DynamicTag {
                expression,
                closing_expression: ByteSpan::default(),
                first_closing_comment: NONE,
                closing_comment_count: 0,
                self_closing: false,
            });
            self.embedded_tokens.push(EmbeddedToken {
                kind: EmbeddedKind::DynamicOpen,
                span: ByteSpan::new(to_u32(start)?, to_u32(end)?),
                owner,
            });
            dynamic_owner = Some(owner);
        } else {
            while self.bytes.get(index).is_some_and(|byte| {
                is_identifier_continue(*byte) || matches!(byte, b'.' | b':' | b'-')
            }) {
                index += 1;
            }
            name_end = index;
            if name_end == name_start {
                return Err(ProjectionError::UnsupportedSyntax {
                    offset: to_u32(start)?,
                    construct: "ambiguous `<` expression",
                });
            }
        }

        let style = !fragment && !dynamic && self.bytes[name_start..name_end] == *b"style";
        let mut self_closing = false;
        if !fragment {
            loop {
                let Some(&byte) = self.bytes.get(index) else {
                    return Err(ProjectionError::UnterminatedSyntax {
                        offset: to_u32(start)?,
                        construct: "JSX opening tag",
                    });
                };
                match byte {
                    b'\'' | b'"' => index = self.skip_quote(index, byte)?,
                    b'{' => index = self.scan_region(index + 1, Some(b'}'))?,
                    b'/' if self.bytes.get(index + 1) == Some(&b'*') => {
                        index = self.skip_block_comment(index)?;
                    }
                    b'/' if self.bytes.get(index + 1) == Some(&b'/') => {
                        index = self.skip_line_comment(index + 2);
                    }
                    b'/' if self.bytes.get(index + 1) == Some(&b'>') => {
                        self_closing = true;
                        index += 2;
                        break;
                    }
                    b'>' => {
                        index += 1;
                        break;
                    }
                    byte if byte.is_ascii_whitespace() => index += 1,
                    byte if is_identifier_start(byte) => {
                        index += 1;
                        while self.bytes.get(index).is_some_and(|byte| {
                            is_identifier_continue(*byte) || matches!(byte, b'-' | b':' | b'.')
                        }) {
                            index += 1;
                        }
                        while self.bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
                            index += 1;
                        }
                        if self.bytes.get(index) == Some(&b'=') {
                            index += 1;
                        }
                    }
                    _ => {
                        return Err(ProjectionError::MalformedSyntax {
                            offset: to_u32(index)?,
                            expected: "a JSX attribute, `>`, or `/>`",
                        });
                    }
                }
            }
        }

        if let Some(owner) = dynamic_owner {
            self.dynamic_tags[owner as usize].self_closing = self_closing;
        }

        if self_closing {
            return Ok(index);
        }

        if style {
            let Some(relative_close) = find_bytes(&self.bytes[index..], b"</style>") else {
                return Err(ProjectionError::UnterminatedSyntax {
                    offset: to_u32(start)?,
                    construct: "inline `<style>` block",
                });
            };
            let close_start = index + relative_close;
            let owner = to_u32(self.style_blocks.len())?;
            let content = ByteSpan::new(to_u32(index)?, to_u32(close_start)?);
            self.style_blocks.push(StyleBlock { content });
            self.embedded_tokens.push(EmbeddedToken {
                kind: EmbeddedKind::StyleContent,
                span: content,
                owner,
            });
            return Ok(close_start + "</style>".len());
        }

        loop {
            let Some(&byte) = self.bytes.get(index) else {
                return Err(ProjectionError::UnterminatedSyntax {
                    offset: to_u32(start)?,
                    construct: "JSX element",
                });
            };
            match byte {
                b'<' if self.bytes.get(index + 1) == Some(&b'/') => {
                    let close_start = index;
                    index += 2;
                    let closing_dynamic = self.bytes.get(index) == Some(&b'{');
                    let (
                        closing_name_start,
                        closing_name_end,
                        closing_expression,
                        closing_identity,
                    ) = if closing_dynamic {
                        let (expression, end) = self.scan_dynamic_expression(index)?;
                        let identity = self.validate_dynamic_expression(expression)?;
                        index = end;
                        (index, index, expression, identity)
                    } else {
                        let closing_name_start = index;
                        while self.bytes.get(index).is_some_and(|byte| {
                            is_identifier_continue(*byte) || matches!(byte, b'.' | b':' | b'-')
                        }) {
                            index += 1;
                        }
                        (
                            closing_name_start,
                            index,
                            ByteSpan::default(),
                            ByteSpan::default(),
                        )
                    };
                    index = self.skip_jsx_tag_trivia(index)?;
                    if self.bytes.get(index) != Some(&b'>') {
                        return Err(ProjectionError::UnterminatedSyntax {
                            offset: to_u32(start)?,
                            construct: "JSX closing tag",
                        });
                    }
                    if fragment {
                        if closing_dynamic || closing_name_start != closing_name_end {
                            return Err(ProjectionError::MalformedSyntax {
                                offset: to_u32(close_start)?,
                                expected: "a fragment closing tag `</>`",
                            });
                        }
                    } else if dynamic {
                        let owner = dynamic_owner.ok_or(ProjectionError::StructuralMismatch)?;
                        if !closing_dynamic
                            || !self.same_dynamic_identity(dynamic_identity, closing_identity)
                        {
                            return Err(ProjectionError::MalformedSyntax {
                                offset: to_u32(close_start)?,
                                expected: "a matching dynamic JSX closing tag",
                            });
                        }
                        let first_closing_comment = to_u32(self.dynamic_comments.len())?;
                        self.collect_dynamic_edge_comments(closing_expression, closing_identity)?;
                        let closing_comment_count = to_u32(self.dynamic_comments.len())?
                            .checked_sub(first_closing_comment)
                            .ok_or(ProjectionError::StructuralMismatch)?;
                        let tag = &mut self.dynamic_tags[owner as usize];
                        tag.closing_expression = closing_expression;
                        tag.first_closing_comment = first_closing_comment;
                        tag.closing_comment_count = closing_comment_count;
                        self.embedded_tokens.push(EmbeddedToken {
                            kind: EmbeddedKind::DynamicClose,
                            span: ByteSpan::new(to_u32(close_start)?, to_u32(index + 1)?),
                            owner,
                        });
                    } else if closing_dynamic
                        || self.bytes[name_start..name_end]
                            != self.bytes[closing_name_start..closing_name_end]
                    {
                        return Err(ProjectionError::MalformedSyntax {
                            offset: to_u32(close_start)?,
                            expected: "a matching JSX closing tag",
                        });
                    }
                    return Ok(index + 1);
                }
                b'<' if self.looks_like_jsx_start(index) => {
                    index = self.scan_jsx_element(index)?;
                }
                b'{' => index = self.scan_region(index + 1, Some(b'}'))?,
                b'@' if self.keyword_at(index, b"if") && self.control_has_header(index, b"if") => {
                    index = self.parse_if(index, ControlContext::JsxChild)?;
                }
                b'@' if self.keyword_at(index, b"for")
                    && self.control_has_header(index, b"for") =>
                {
                    index = self.parse_for(index, ControlContext::JsxChild)?;
                }
                b'@' if self.keyword_at(index, b"switch")
                    && self.control_has_header(index, b"switch") =>
                {
                    index = self.parse_switch(index, ControlContext::JsxChild)?;
                }
                b'@' if self.keyword_at(index, b"try") && self.control_has_body(index, b"try") => {
                    index = self.parse_try(index, ControlContext::JsxChild)?;
                }
                b'@' if self.bytes.get(index + 1) == Some(&b'{') => {
                    return Err(ProjectionError::UnsupportedSyntax {
                        offset: to_u32(index)?,
                        construct: "code block directly inside JSX text",
                    });
                }
                b'@' => {
                    if self.keyword_at(index, b"else")
                        || self.keyword_at(index, b"empty")
                        || self.keyword_at(index, b"case")
                        || self.keyword_at(index, b"default")
                        || self.keyword_at(index, b"pending")
                        || self.keyword_at(index, b"catch")
                    {
                        return Err(ProjectionError::MalformedSyntax {
                            offset: to_u32(index)?,
                            expected: "an owning TSRX control",
                        });
                    }
                    if jsx_text_looks_structural(self.bytes, index)
                        && let Some(construct) = unsupported_at_construct(self.bytes, index)
                    {
                        return Err(ProjectionError::UnsupportedSyntax {
                            offset: to_u32(index)?,
                            construct,
                        });
                    }
                    index += 1;
                }
                _ => index += 1,
            }
        }
    }

    fn scan_dynamic_expression(&self, open: usize) -> Result<(ByteSpan, usize), ProjectionError> {
        if self.bytes.get(open) != Some(&b'{') {
            return Err(ProjectionError::MalformedSyntax {
                offset: to_u32(open)?,
                expected: "a dynamic JSX tag expression",
            });
        }
        let mut index = open + 1;
        let mut braces = 1usize;
        let mut can_start_expression = true;
        while index < self.bytes.len() {
            match self.bytes[index] {
                b'\'' | b'"' => {
                    index = self.skip_quote(index, self.bytes[index])?;
                    can_start_expression = false;
                }
                b'`' => {
                    index = self.skip_template_raw(index, self.bytes.len())?;
                    can_start_expression = false;
                }
                b'/' if self.bytes.get(index + 1) == Some(&b'/') => {
                    index = self.skip_line_comment(index + 2);
                }
                b'/' if self.bytes.get(index + 1) == Some(&b'*') => {
                    index = self.skip_block_comment(index)?;
                }
                b'/' if can_start_expression => {
                    index = self.skip_regex(index)?;
                    can_start_expression = false;
                }
                b'{' => {
                    braces += 1;
                    index += 1;
                    can_start_expression = true;
                }
                b'}' => {
                    braces -= 1;
                    if braces == 0 {
                        return Ok((ByteSpan::new(to_u32(open + 1)?, to_u32(index)?), index + 1));
                    }
                    index += 1;
                    can_start_expression = false;
                }
                byte if is_identifier_start(byte) => {
                    index = self.skip_identifier(index);
                    can_start_expression = false;
                }
                byte if byte.is_ascii_digit() => {
                    index = self.skip_number(index);
                    can_start_expression = false;
                }
                b')' | b']' => {
                    index += 1;
                    can_start_expression = false;
                }
                b'.' if self.bytes.get(index + 1) != Some(&b'.') => {
                    index += 1;
                    can_start_expression = false;
                }
                _ => {
                    index += 1;
                    can_start_expression = true;
                }
            }
        }
        Err(ProjectionError::UnterminatedSyntax {
            offset: to_u32(open)?,
            construct: "dynamic JSX tag expression",
        })
    }

    fn skip_jsx_tag_trivia(&self, mut index: usize) -> Result<usize, ProjectionError> {
        loop {
            while self.bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
                index += 1;
            }
            if self.bytes.get(index..index + 2) == Some(b"/*") {
                index = self.skip_block_comment(index)?;
            } else if self.bytes.get(index..index + 2) == Some(b"//") {
                index = self.skip_line_comment(index + 2);
            } else {
                return Ok(index);
            }
        }
    }

    fn validate_dynamic_expression(&self, span: ByteSpan) -> Result<ByteSpan, ProjectionError> {
        let identity = self.dynamic_identity_range(span)?;
        if identity.is_empty() {
            return Err(ProjectionError::MalformedSyntax {
                offset: span.start,
                expected: "a valid dynamic JSX tag expression",
            });
        }
        Ok(identity)
    }

    fn same_dynamic_identity(&self, opening: ByteSpan, closing: ByteSpan) -> bool {
        self.bytes[opening.start as usize..opening.end as usize]
            == self.bytes[closing.start as usize..closing.end as usize]
    }

    fn dynamic_identity_range(&self, span: ByteSpan) -> Result<ByteSpan, ProjectionError> {
        let mut index = span.start as usize;
        let end = span.end as usize;
        let mut can_start_expression = true;
        let mut first_start = None;
        let mut last_end = span.start as usize;
        let mut previous_end = span.start as usize;
        let mut in_leading_prefix = true;
        let mut leading_unclosed = 0usize;
        let mut other_depth = 0usize;
        let mut trailing_outer_closures = 0usize;
        let mut trailing_inner_end = span.start as usize;
        while let Some((token_start, token_end)) =
            self.next_dynamic_identity_token(&mut index, end, &mut can_start_expression)?
        {
            first_start.get_or_insert(token_start);
            let byte = self.bytes[token_start];
            let leading_open = in_leading_prefix && byte == b'(';
            if leading_open {
                leading_unclosed += 1;
                trailing_outer_closures = 0;
            } else {
                in_leading_prefix = false;
                let closes_leading = if byte == b'(' {
                    other_depth += 1;
                    false
                } else if byte == b')' && other_depth > 0 {
                    other_depth -= 1;
                    false
                } else if byte == b')' && leading_unclosed > 0 {
                    leading_unclosed -= 1;
                    true
                } else {
                    false
                };
                if closes_leading {
                    if trailing_outer_closures == 0 {
                        trailing_inner_end = previous_end;
                    }
                    trailing_outer_closures += 1;
                } else {
                    trailing_outer_closures = 0;
                }
            }
            previous_end = token_end;
            last_end = token_end;
        }
        let Some(first_start) = first_start else {
            return Ok(ByteSpan::new(span.start, span.start));
        };
        if trailing_outer_closures == 0 {
            return Ok(ByteSpan::new(to_u32(first_start)?, to_u32(last_end)?));
        }

        let mut normalized_start = first_start;
        let mut prefix_index = span.start as usize;
        let mut prefix_can_start_expression = true;
        for _ in 0..trailing_outer_closures {
            let Some((token_start, _)) = self.next_dynamic_identity_token(
                &mut prefix_index,
                end,
                &mut prefix_can_start_expression,
            )?
            else {
                return Err(ProjectionError::StructuralMismatch);
            };
            if self.bytes[token_start] != b'(' {
                return Err(ProjectionError::StructuralMismatch);
            }
        }
        if let Some((token_start, _)) = self.next_dynamic_identity_token(
            &mut prefix_index,
            end,
            &mut prefix_can_start_expression,
        )? {
            normalized_start = token_start;
        }
        if normalized_start > trailing_inner_end {
            normalized_start = trailing_inner_end;
        }
        Ok(ByteSpan::new(
            to_u32(normalized_start)?,
            to_u32(trailing_inner_end)?,
        ))
    }

    fn next_dynamic_identity_token(
        &self,
        index: &mut usize,
        end: usize,
        can_start_expression: &mut bool,
    ) -> Result<Option<(usize, usize)>, ProjectionError> {
        while *index < end {
            if self.bytes[*index].is_ascii_whitespace() {
                *index += 1;
                continue;
            }
            if self.bytes.get(*index..*index + 2) == Some(b"//") {
                *index = self.skip_line_comment(*index + 2).min(end);
                continue;
            }
            if self.bytes.get(*index..*index + 2) == Some(b"/*") {
                *index = self.skip_block_comment(*index)?.min(end);
                continue;
            }
            let token_start = *index;
            match self.bytes[*index] {
                b'\'' | b'"' => {
                    *index = self.skip_quote(*index, self.bytes[*index])?.min(end);
                    *can_start_expression = false;
                }
                b'`' => {
                    *index = self.skip_template_raw(*index, end)?;
                    *can_start_expression = false;
                }
                b'/' if *can_start_expression => {
                    *index = self.skip_regex(*index)?.min(end);
                    *can_start_expression = false;
                }
                b'(' => {
                    *index += 1;
                    *can_start_expression = true;
                }
                b')' => {
                    *index += 1;
                    *can_start_expression = false;
                }
                byte if is_identifier_start(byte) => {
                    *index = self.skip_identifier(*index);
                    *can_start_expression = matches!(
                        &self.bytes[token_start..*index],
                        b"return"
                            | b"throw"
                            | b"case"
                            | b"delete"
                            | b"void"
                            | b"typeof"
                            | b"new"
                            | b"yield"
                            | b"await"
                            | b"in"
                            | b"of"
                            | b"instanceof"
                    );
                }
                byte if byte.is_ascii_digit() => {
                    *index = self.skip_number(*index);
                    *can_start_expression = false;
                }
                b']' | b'}' | b'.' => {
                    *index += 1;
                    *can_start_expression = false;
                }
                _ => {
                    *index += 1;
                    *can_start_expression = true;
                }
            }
            return Ok(Some((token_start, *index)));
        }
        Ok(None)
    }

    fn collect_dynamic_edge_comments(
        &mut self,
        expression: ByteSpan,
        identity: ByteSpan,
    ) -> Result<(), ProjectionError> {
        if identity.start < expression.start || identity.end > expression.end {
            return Err(ProjectionError::StructuralMismatch);
        }
        self.collect_dynamic_comments_in(expression.start as usize, identity.start as usize)?;
        self.collect_dynamic_comments_in(identity.end as usize, expression.end as usize)
    }

    fn collect_dynamic_comments_in(
        &mut self,
        mut index: usize,
        end: usize,
    ) -> Result<(), ProjectionError> {
        while index < end {
            if self.bytes.get(index..index + 2) == Some(b"//") {
                let comment_end = self.skip_line_comment(index + 2).min(end);
                self.dynamic_comments
                    .push(ByteSpan::new(to_u32(index)?, to_u32(comment_end)?));
                index = comment_end;
            } else if self.bytes.get(index..index + 2) == Some(b"/*") {
                let comment_end = self.skip_block_comment(index)?;
                if comment_end > end {
                    return Err(ProjectionError::StructuralMismatch);
                }
                self.dynamic_comments
                    .push(ByteSpan::new(to_u32(index)?, to_u32(comment_end)?));
                index = comment_end;
            } else {
                index += 1;
            }
        }
        Ok(())
    }

    fn begin_node(
        &mut self,
        kind: ControlKind,
        context: ControlContext,
        start: usize,
    ) -> Result<u32, ProjectionError> {
        let index = to_u32(self.nodes.len())?;
        let parent = self.parents.last().copied().unwrap_or(NONE);
        self.nodes.push(SyntaxNode {
            kind,
            context,
            span: ByteSpan::new(to_u32(start)?, to_u32(start)?),
            parent,
            first_child: NONE,
            last_child: NONE,
            next_sibling: NONE,
            first_clause: NONE,
            last_clause: NONE,
        });
        if parent == NONE {
            if self.first_root == NONE {
                self.first_root = index;
            } else {
                self.nodes[self.last_root as usize].next_sibling = index;
            }
            self.last_root = index;
        } else {
            let parent_index = parent as usize;
            let previous = self.nodes[parent_index].last_child;
            if previous == NONE {
                self.nodes[parent_index].first_child = index;
            } else {
                self.nodes[previous as usize].next_sibling = index;
            }
            self.nodes[parent_index].last_child = index;
        }
        Ok(index)
    }

    fn add_clause(
        &mut self,
        node: u32,
        role: ClauseRole,
        keyword_start: usize,
        header: ByteSpan,
        body: ByteSpan,
        for_header: ForHeader,
    ) -> Result<u32, ProjectionError> {
        self.add_clause_with_bindings(node, role, keyword_start, header, body, for_header, 0)
    }

    #[allow(clippy::too_many_arguments)]
    fn add_clause_with_bindings(
        &mut self,
        node: u32,
        role: ClauseRole,
        keyword_start: usize,
        header: ByteSpan,
        body: ByteSpan,
        for_header: ForHeader,
        bindings: u8,
    ) -> Result<u32, ProjectionError> {
        let index = to_u32(self.clauses.len())?;
        self.clauses.push(Clause {
            role,
            keyword: ByteSpan::new(to_u32(keyword_start)?, to_u32(keyword_start + 1)?),
            header,
            body,
            for_header,
            bindings,
            next: NONE,
        });
        let node_index = node as usize;
        let previous = self.nodes[node_index].last_clause;
        if previous == NONE {
            self.nodes[node_index].first_clause = index;
        } else {
            self.clauses[previous as usize].next = index;
        }
        self.nodes[node_index].last_clause = index;
        Ok(index)
    }

    fn push_token(&mut self, kind: StructuralKind, index: usize) -> Result<(), ProjectionError> {
        let start = to_u32(index)?;
        self.tokens.push(StructuralToken {
            kind,
            span: ByteSpan::new(start, start + 1),
            owner: self.parents.last().copied().unwrap_or(NONE),
        });
        Ok(())
    }

    fn checkpoint(&self) -> Checkpoint {
        let parent = self.parents.last().copied().map(|index| {
            let node = self.nodes[index as usize];
            (index as usize, node.first_child, node.last_child)
        });
        Checkpoint {
            tokens: self.tokens.len(),
            nodes: self.nodes.len(),
            clauses: self.clauses.len(),
            embedded_tokens: self.embedded_tokens.len(),
            dynamic_tags: self.dynamic_tags.len(),
            dynamic_comments: self.dynamic_comments.len(),
            style_blocks: self.style_blocks.len(),
            first_root: self.first_root,
            last_root: self.last_root,
            parent,
        }
    }

    fn rollback(&mut self, checkpoint: Checkpoint) {
        self.tokens.truncate(checkpoint.tokens);
        self.nodes.truncate(checkpoint.nodes);
        self.clauses.truncate(checkpoint.clauses);
        self.embedded_tokens.truncate(checkpoint.embedded_tokens);
        self.dynamic_tags.truncate(checkpoint.dynamic_tags);
        self.dynamic_comments.truncate(checkpoint.dynamic_comments);
        self.style_blocks.truncate(checkpoint.style_blocks);
        self.first_root = checkpoint.first_root;
        self.last_root = checkpoint.last_root;
        if let Some((index, first_child, last_child)) = checkpoint.parent {
            self.nodes[index].first_child = first_child;
            self.nodes[index].last_child = last_child;
            if last_child != NONE {
                self.nodes[last_child as usize].next_sibling = NONE;
            }
        } else if self.last_root != NONE {
            self.nodes[self.last_root as usize].next_sibling = NONE;
        }
    }

    fn control_has_header(&self, start: usize, keyword: &[u8]) -> bool {
        let mut index = Self::after_keyword(start, keyword);
        index = self.skip_ascii_whitespace(index, self.bytes.len());
        if keyword == b"for" && self.bare_keyword_at(index, b"await") {
            index = self
                .skip_ascii_whitespace(Self::after_bare_keyword(index, b"await"), self.bytes.len());
        }
        self.bytes.get(index) == Some(&b'(')
    }

    fn control_has_body(&self, start: usize, keyword: &[u8]) -> bool {
        self.skip_trivia(Self::after_keyword(start, keyword))
            .is_ok_and(|index| self.bytes.get(index) == Some(&b'{'))
    }

    fn code_context(&self, start: usize) -> ControlContext {
        let mut index = start;
        loop {
            while index > 0 && self.bytes[index - 1].is_ascii_whitespace() {
                index -= 1;
            }
            if index >= 2 && self.bytes.get(index - 2..index) == Some(b"*/") {
                let Some(comment_start) = self.bytes[..index - 2]
                    .windows(2)
                    .rposition(|window| window == b"/*")
                else {
                    break;
                };
                index = comment_start;
                continue;
            }
            let line_start = self.bytes[..index]
                .iter()
                .rposition(|byte| matches!(byte, b'\n' | b'\r'))
                .map_or(0, |position| position + 1);
            let line = &self.bytes[line_start..index];
            let first = line
                .iter()
                .position(|byte| !byte.is_ascii_whitespace())
                .unwrap_or(line.len());
            if line.get(first..first + 2) == Some(b"//") {
                index = line_start;
                continue;
            }
            break;
        }
        if index == 0 || matches!(self.bytes[index - 1], b'{' | b'}' | b';') {
            ControlContext::Statement
        } else {
            ControlContext::Expression
        }
    }

    fn committed_jsx_opening(&self, start: usize) -> bool {
        if self.bytes.get(start + 1) == Some(&b'{') {
            return true;
        }
        if self.bytes.get(start + 1) == Some(&b'>') {
            return true;
        }
        let mut index = start + 1;
        if !self
            .bytes
            .get(index)
            .is_some_and(|byte| is_identifier_start(*byte))
        {
            return false;
        }
        while self
            .bytes
            .get(index)
            .is_some_and(|byte| is_identifier_continue(*byte) || matches!(byte, b'.' | b':' | b'-'))
        {
            index += 1;
        }
        self.bytes.get(index).is_some_and(|byte| {
            byte.is_ascii_whitespace()
                || *byte == b'>'
                || (*byte == b'/' && self.bytes.get(index + 1) == Some(&b'*'))
                || (*byte == b'/' && self.bytes.get(index + 1) == Some(&b'>'))
        })
    }

    fn keyword_at(&self, index: usize, keyword: &[u8]) -> bool {
        self.bytes.get(index) == Some(&b'@')
            && self.bytes.get(index + 1..index + 1 + keyword.len()) == Some(keyword)
            && keyword_boundary(self.bytes.get(index + 1 + keyword.len()))
    }

    fn bare_keyword_at(&self, index: usize, keyword: &[u8]) -> bool {
        self.bytes.get(index..index + keyword.len()) == Some(keyword)
            && keyword_boundary(self.bytes.get(index + keyword.len()))
            && (index == 0 || !is_identifier_continue(self.bytes[index - 1]))
    }

    const fn after_keyword(index: usize, keyword: &[u8]) -> usize {
        index + 1 + keyword.len()
    }

    const fn after_bare_keyword(index: usize, keyword: &[u8]) -> usize {
        index + keyword.len()
    }

    fn skip_trivia(&self, mut index: usize) -> Result<usize, ProjectionError> {
        loop {
            while self.bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
                index += 1;
            }
            if self.bytes.get(index..index + 2) == Some(b"//") {
                index = self.skip_line_comment(index + 2);
            } else if self.bytes.get(index..index + 2) == Some(b"/*") {
                index = self.skip_block_comment(index)?;
            } else {
                return Ok(index);
            }
        }
    }

    fn skip_ascii_whitespace(&self, mut index: usize, end: usize) -> usize {
        while index < end && self.bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        index
    }

    fn skip_quote(&self, start: usize, quote: u8) -> Result<usize, ProjectionError> {
        let mut index = start + 1;
        let mut escaped = false;
        while index < self.bytes.len() {
            let byte = self.bytes[index];
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == quote {
                return Ok(index + 1);
            } else if matches!(byte, b'\n' | b'\r') {
                break;
            }
            index += 1;
        }
        Err(ProjectionError::UnterminatedSyntax {
            offset: to_u32(start)?,
            construct: "quoted string",
        })
    }

    fn skip_line_comment(&self, mut index: usize) -> usize {
        while index < self.bytes.len() && !matches!(self.bytes[index], b'\n' | b'\r') {
            index += 1;
        }
        index
    }

    fn skip_block_comment(&self, start: usize) -> Result<usize, ProjectionError> {
        let mut index = start + 2;
        while index + 1 < self.bytes.len() {
            if self.bytes[index..index + 2] == *b"*/" {
                return Ok(index + 2);
            }
            index += 1;
        }
        Err(ProjectionError::UnterminatedSyntax {
            offset: to_u32(start)?,
            construct: "block comment",
        })
    }

    fn skip_regex(&self, start: usize) -> Result<usize, ProjectionError> {
        let mut index = start + 1;
        let mut escaped = false;
        let mut in_class = false;
        while index < self.bytes.len() {
            let byte = self.bytes[index];
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'[' {
                in_class = true;
            } else if byte == b']' {
                in_class = false;
            } else if byte == b'/' && !in_class {
                index += 1;
                while self
                    .bytes
                    .get(index)
                    .is_some_and(|byte| is_identifier_continue(*byte))
                {
                    index += 1;
                }
                return Ok(index);
            } else if matches!(byte, b'\n' | b'\r') {
                break;
            }
            index += 1;
        }
        Err(ProjectionError::UnterminatedSyntax {
            offset: to_u32(start)?,
            construct: "regular expression literal",
        })
    }

    fn skip_number(&self, mut index: usize) -> usize {
        while self
            .bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'))
        {
            index += 1;
        }
        index
    }

    fn skip_identifier(&self, mut index: usize) -> usize {
        index += 1;
        while self
            .bytes
            .get(index)
            .is_some_and(|byte| is_identifier_continue(*byte))
        {
            index += 1;
        }
        index
    }

    fn looks_like_jsx_start(&self, index: usize) -> bool {
        self.bytes
            .get(index + 1)
            .is_some_and(|byte| is_identifier_start(*byte) || matches!(byte, b'>' | b'{'))
    }
}

fn trim_ascii_end(bytes: &[u8], start: usize, mut end: usize) -> usize {
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    end
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn previous_significant_byte(bytes: &[u8], before: usize) -> Option<u8> {
    bytes[..before]
        .iter()
        .rfind(|byte| !byte.is_ascii_whitespace())
        .copied()
}

fn unsupported_at_construct(bytes: &[u8], index: usize) -> Option<&'static str> {
    const UNSUPPORTED: [(&[u8], &str); 1] = [(b"await", "@await control flow")];
    UNSUPPORTED.iter().find_map(|(keyword, construct)| {
        let end = index + 1 + keyword.len();
        (bytes.get(index + 1..end) == Some(*keyword) && keyword_boundary(bytes.get(end)))
            .then_some(*construct)
    })
}

fn jsx_text_looks_structural(bytes: &[u8], index: usize) -> bool {
    [b"if".as_slice(), b"for", b"switch", b"try"]
        .iter()
        .any(|keyword| {
            let end = index + 1 + keyword.len();
            if bytes.get(index + 1..end) != Some(*keyword) || !keyword_boundary(bytes.get(end)) {
                return false;
            }
            bytes[end..]
                .iter()
                .find(|byte| !byte.is_ascii_whitespace())
                .copied()
                == Some(b'(')
                || (*keyword == b"try"
                    && bytes[end..]
                        .iter()
                        .find(|byte| !byte.is_ascii_whitespace())
                        .copied()
                        == Some(b'{'))
        })
}

pub(crate) fn source_fingerprint(bytes: &[u8]) -> u128 {
    let mut first = 0x9e37_79b1_85eb_ca87_u64 ^ bytes.len() as u64;
    let mut second = 0xc2b2_ae3d_27d4_eb4f_u64 ^ (bytes.len() as u64).rotate_left(17);
    for chunk in bytes.chunks(8) {
        let mut word = [0_u8; 8];
        word[..chunk.len()].copy_from_slice(chunk);
        let value = u64::from_le_bytes(word);
        first = (first ^ value)
            .wrapping_mul(0x9e37_79b1_85eb_ca87)
            .rotate_left(27);
        second = (second ^ value.rotate_left(31))
            .wrapping_mul(0xc2b2_ae3d_27d4_eb4f)
            .rotate_left(33);
    }
    u128::from(first) << 64 | u128::from(second)
}

fn keyword_boundary(next: Option<&u8>) -> bool {
    next.is_none_or(|byte| !is_identifier_continue(*byte))
}

pub(crate) const fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$')
}

pub(crate) const fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$')
}
