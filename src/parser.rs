//! State Machine Parser for Fool Shell
//! Implements a DFA-based parser for command line input

use std::fmt;

/// Parser states for the state machine
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParserState {
    /// Initial state, waiting for input
    Idle,
    /// Parsing the command name
    CommandStart,
    /// Parsing command arguments
    Argument,
    /// Inside single quotes
    SingleQuote,
    /// Inside double quotes
    DoubleQuote,
    /// After a pipe character, ready for next command
    Pipe,
    /// After > for output redirection
    RedirectOut,
    /// After >> for append redirection
    RedirectAppend,
    /// After < for input redirection
    RedirectIn,
    /// AI mode triggered by prefix (default: !)
    #[allow(dead_code)] // Reserved for future state-based AI mode handling
    AIMode,
    /// Escape sequence (backslash)
    Escape,
}

impl fmt::Display for ParserState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParserState::Idle => write!(f, "Idle"),
            ParserState::CommandStart => write!(f, "Command"),
            ParserState::Argument => write!(f, "Argument"),
            ParserState::SingleQuote => write!(f, "SingleQuote"),
            ParserState::DoubleQuote => write!(f, "DoubleQuote"),
            ParserState::Pipe => write!(f, "Pipe"),
            ParserState::RedirectOut => write!(f, "RedirectOut"),
            ParserState::RedirectAppend => write!(f, "RedirectAppend"),
            ParserState::RedirectIn => write!(f, "RedirectIn"),
            ParserState::AIMode => write!(f, "AIMode"),
            ParserState::Escape => write!(f, "Escape"),
        }
    }
}

/// Represents a single command with its arguments
#[derive(Debug, Clone, Default)]
pub struct Command {
    pub program: String,
    pub args: Vec<String>,
    pub stdin_redirect: Option<String>,
    pub stdout_redirect: Option<String>,
    pub stdout_append: bool,
}

impl Command {
    #[allow(dead_code)] // Public API for command construction
    pub fn new(program: String) -> Self {
        Self {
            program,
            args: Vec::new(),
            stdin_redirect: None,
            stdout_redirect: None,
            stdout_append: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.program.is_empty()
    }
}

/// Result of parsing a command line
#[derive(Debug, Clone)]
pub enum ParseResult {
    /// Regular shell command(s) connected by pipes
    Commands(Vec<Command>),
    /// AI query (triggered by prefix)
    AIQuery(String),
    /// Empty input
    Empty,
    /// Parse error
    Error(String),
}

/// State machine parser for shell commands
pub struct Parser {
    ai_trigger: String,
}

impl Parser {
    pub fn new(ai_trigger: String) -> Self {
        Self { ai_trigger }
    }

    /// Parse a command line input
    pub fn parse(&self, input: &str) -> ParseResult {
        let trimmed = input.trim();

        if trimmed.is_empty() {
            return ParseResult::Empty;
        }

        // Check for AI trigger at the start
        if trimmed.starts_with(&self.ai_trigger) {
            let query = trimmed[self.ai_trigger.len()..].trim().to_string();
            return ParseResult::AIQuery(query);
        }

        // Parse as shell command
        self.parse_commands(trimmed)
    }

    fn parse_commands(&self, input: &str) -> ParseResult {
        let mut commands: Vec<Command> = Vec::new();
        let mut current_command = Command::default();
        let mut current_token = String::new();
        let mut state = ParserState::Idle;
        let mut prev_state = ParserState::Idle;

        let chars: Vec<char> = input.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let c = chars[i];

            match state {
                ParserState::Idle | ParserState::CommandStart | ParserState::Argument => {
                    match c {
                        ' ' | '\t' => {
                            if !current_token.is_empty() {
                                self.add_token(&mut current_command, &current_token, &state);
                                current_token.clear();
                                state = ParserState::Argument;
                            }
                        }
                        '\'' => {
                            prev_state = state.clone();
                            state = ParserState::SingleQuote;
                        }
                        '"' => {
                            prev_state = state.clone();
                            state = ParserState::DoubleQuote;
                        }
                        '\\' => {
                            prev_state = state.clone();
                            state = ParserState::Escape;
                        }
                        '|' => {
                            if !current_token.is_empty() {
                                self.add_token(&mut current_command, &current_token, &state);
                                current_token.clear();
                            }
                            if !current_command.is_empty() {
                                commands.push(current_command);
                                current_command = Command::default();
                            }
                            state = ParserState::Pipe;
                        }
                        '>' => {
                            if !current_token.is_empty() {
                                self.add_token(&mut current_command, &current_token, &state);
                                current_token.clear();
                            }
                            // Check for >>
                            if i + 1 < chars.len() && chars[i + 1] == '>' {
                                state = ParserState::RedirectAppend;
                                i += 1;
                            } else {
                                state = ParserState::RedirectOut;
                            }
                        }
                        '<' => {
                            if !current_token.is_empty() {
                                self.add_token(&mut current_command, &current_token, &state);
                                current_token.clear();
                            }
                            state = ParserState::RedirectIn;
                        }
                        _ => {
                            current_token.push(c);
                            if state == ParserState::Idle {
                                state = ParserState::CommandStart;
                            }
                        }
                    }
                }
                ParserState::SingleQuote => {
                    if c == '\'' {
                        state = prev_state.clone();
                    } else {
                        current_token.push(c);
                    }
                }
                ParserState::DoubleQuote => {
                    if c == '"' {
                        state = prev_state.clone();
                    } else if c == '\\' && i + 1 < chars.len() {
                        // Handle escape in double quotes
                        let next = chars[i + 1];
                        if next == '"' || next == '\\' || next == '$' {
                            current_token.push(next);
                            i += 1;
                        } else {
                            current_token.push(c);
                        }
                    } else {
                        current_token.push(c);
                    }
                }
                ParserState::Escape => {
                    current_token.push(c);
                    state = prev_state.clone();
                }
                ParserState::Pipe => {
                    if !c.is_whitespace() {
                        current_token.push(c);
                        state = ParserState::CommandStart;
                    }
                }
                ParserState::RedirectOut | ParserState::RedirectAppend => {
                    match c {
                        ' ' | '\t' => {
                            if !current_token.is_empty() {
                                current_command.stdout_redirect = Some(current_token.clone());
                                current_command.stdout_append = state == ParserState::RedirectAppend;
                                current_token.clear();
                                state = ParserState::Argument;
                            }
                        }
                        '\'' => {
                            prev_state = state.clone();
                            state = ParserState::SingleQuote;
                        }
                        '"' => {
                            prev_state = state.clone();
                            state = ParserState::DoubleQuote;
                        }
                        '\\' => {
                            prev_state = state.clone();
                            state = ParserState::Escape;
                        }
                        _ => {
                            current_token.push(c);
                        }
                    }
                }
                ParserState::RedirectIn => {
                    match c {
                        ' ' | '\t' => {
                            if !current_token.is_empty() {
                                current_command.stdin_redirect = Some(current_token.clone());
                                current_token.clear();
                                state = ParserState::Argument;
                            }
                        }
                        '\'' => {
                            prev_state = state.clone();
                            state = ParserState::SingleQuote;
                        }
                        '"' => {
                            prev_state = state.clone();
                            state = ParserState::DoubleQuote;
                        }
                        '\\' => {
                            prev_state = state.clone();
                            state = ParserState::Escape;
                        }
                        _ => {
                            current_token.push(c);
                        }
                    }
                }
                ParserState::AIMode => {
                    // Should not reach here in normal parsing
                    unreachable!()
                }
            }
            i += 1;
        }

        // Handle remaining token
        if !current_token.is_empty() {
            match state {
                ParserState::RedirectOut | ParserState::RedirectAppend => {
                    current_command.stdout_redirect = Some(current_token);
                    current_command.stdout_append = state == ParserState::RedirectAppend;
                    state = ParserState::Argument; // Update state to show we processed the redirect
                }
                ParserState::RedirectIn => {
                    current_command.stdin_redirect = Some(current_token);
                    state = ParserState::Argument; // Update state to show we processed the redirect
                }
                ParserState::Escape => {
                    // M-06 FIX: Trailing backslash with content - add content and mark as escape error
                    self.add_token(&mut current_command, &current_token, &prev_state);
                    // State remains Escape for error check below
                }
                _ => {
                    self.add_token(&mut current_command, &current_token, &state);
                }
            }
        }

        // Add last command if not empty
        if !current_command.is_empty() {
            commands.push(current_command);
        }

        // M-06 FIX: Check for trailing backslash (incomplete escape sequence)
        if state == ParserState::Escape {
            return ParseResult::Error("Syntax error: trailing backslash".to_string());
        }

        // Check for unclosed quotes
        if state == ParserState::SingleQuote || state == ParserState::DoubleQuote {
            return ParseResult::Error("Unclosed quote".to_string());
        }

        // Validate final state: check for incomplete pipelines or redirections
        match state {
            ParserState::Pipe => {
                return ParseResult::Error("Syntax error: pipe without following command".to_string());
            }
            ParserState::RedirectOut | ParserState::RedirectAppend => {
                return ParseResult::Error("Syntax error: output redirection without file".to_string());
            }
            ParserState::RedirectIn => {
                return ParseResult::Error("Syntax error: input redirection without file".to_string());
            }
            _ => {}
        }

        if commands.is_empty() {
            ParseResult::Empty
        } else {
            ParseResult::Commands(commands)
        }
    }

    fn add_token(&self, command: &mut Command, token: &str, state: &ParserState) {
        if command.program.is_empty() || *state == ParserState::CommandStart {
            command.program = token.to_string();
        } else {
            command.args.push(token.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_command() {
        let parser = Parser::new("!".to_string());
        match parser.parse("ls -la") {
            ParseResult::Commands(cmds) => {
                assert_eq!(cmds.len(), 1);
                assert_eq!(cmds[0].program, "ls");
                assert_eq!(cmds[0].args, vec!["-la"]);
            }
            _ => panic!("Expected Commands"),
        }
    }

    #[test]
    fn test_pipe() {
        let parser = Parser::new("!".to_string());
        match parser.parse("cat file.txt | grep pattern") {
            ParseResult::Commands(cmds) => {
                assert_eq!(cmds.len(), 2);
                assert_eq!(cmds[0].program, "cat");
                assert_eq!(cmds[1].program, "grep");
            }
            _ => panic!("Expected Commands"),
        }
    }

    #[test]
    fn test_ai_trigger() {
        let parser = Parser::new("!".to_string());
        match parser.parse("! how to unzip a tar.gz file") {
            ParseResult::AIQuery(query) => {
                assert_eq!(query, "how to unzip a tar.gz file");
            }
            _ => panic!("Expected AIQuery"),
        }
    }

    #[test]
    fn test_redirect() {
        let parser = Parser::new("!".to_string());
        let result = parser.parse("echo hello > output.txt");
        match result {
            ParseResult::Commands(cmds) => {
                assert_eq!(cmds.len(), 1);
                assert_eq!(cmds[0].program, "echo");
                assert_eq!(cmds[0].stdout_redirect, Some("output.txt".to_string()));
                assert!(!cmds[0].stdout_append);
            }
            ParseResult::Error(e) => panic!("Parse error: {}", e),
            _ => panic!("Expected Commands, got: {:?}", result),
        }
    }

    #[test]
    fn test_quotes() {
        let parser = Parser::new("!".to_string());
        match parser.parse("echo 'hello world'") {
            ParseResult::Commands(cmds) => {
                assert_eq!(cmds.len(), 1);
                assert_eq!(cmds[0].args, vec!["hello world"]);
            }
            _ => panic!("Expected Commands"),
        }
    }

    #[test]
    fn test_empty() {
        let parser = Parser::new("!".to_string());
        match parser.parse("   ") {
            ParseResult::Empty => {}
            _ => panic!("Expected Empty"),
        }
    }

    #[test]
    fn test_incomplete_pipe() {
        let parser = Parser::new("!".to_string());
        match parser.parse("ls |") {
            ParseResult::Error(e) => {
                assert!(e.contains("pipe"));
            }
            _ => panic!("Expected Error for incomplete pipe"),
        }
    }

    #[test]
    fn test_incomplete_redirect_out() {
        let parser = Parser::new("!".to_string());
        match parser.parse("echo test >") {
            ParseResult::Error(e) => {
                assert!(e.contains("redirection"));
            }
            _ => panic!("Expected Error for incomplete redirect"),
        }
    }

    #[test]
    fn test_incomplete_redirect_in() {
        let parser = Parser::new("!".to_string());
        match parser.parse("cat <") {
            ParseResult::Error(e) => {
                assert!(e.contains("redirection"));
            }
            _ => panic!("Expected Error for incomplete redirect"),
        }
    }

    #[test]
    fn test_append_redirect() {
        let parser = Parser::new("!".to_string());
        match parser.parse("echo test >> file.txt") {
            ParseResult::Commands(cmds) => {
                assert_eq!(cmds.len(), 1);
                assert_eq!(cmds[0].stdout_redirect, Some("file.txt".to_string()));
                assert!(cmds[0].stdout_append);
            }
            _ => panic!("Expected Commands"),
        }
    }

    #[test]
    fn test_redirect_with_quoted_filename() {
        let parser = Parser::new("!".to_string());
        // Test double quotes
        match parser.parse("echo hi > \"build logs/output.txt\"") {
            ParseResult::Commands(cmds) => {
                assert_eq!(cmds.len(), 1);
                assert_eq!(cmds[0].stdout_redirect, Some("build logs/output.txt".to_string()));
            }
            _ => panic!("Expected Commands"),
        }

        // Test single quotes
        match parser.parse("echo hi > 'my file.txt'") {
            ParseResult::Commands(cmds) => {
                assert_eq!(cmds.len(), 1);
                assert_eq!(cmds[0].stdout_redirect, Some("my file.txt".to_string()));
            }
            _ => panic!("Expected Commands"),
        }

        // Test input redirect with quotes
        match parser.parse("cat < \"input file.txt\"") {
            ParseResult::Commands(cmds) => {
                assert_eq!(cmds.len(), 1);
                assert_eq!(cmds[0].stdin_redirect, Some("input file.txt".to_string()));
            }
            _ => panic!("Expected Commands"),
        }
    }

    #[test]
    fn test_trailing_backslash() {
        let parser = Parser::new("!".to_string());
        // M-06: Trailing backslash should return an error
        match parser.parse("echo test\\") {
            ParseResult::Error(e) => {
                assert!(e.contains("backslash"), "Expected backslash error, got: {}", e);
            }
            other => panic!("Expected Error for trailing backslash, got: {:?}", other),
        }

        // Single backslash should also be an error
        match parser.parse("\\") {
            ParseResult::Error(e) => {
                assert!(e.contains("backslash"), "Expected backslash error, got: {}", e);
            }
            other => panic!("Expected Error for single backslash, got: {:?}", other),
        }
    }
}
