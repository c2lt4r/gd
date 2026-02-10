/// AST pretty printer - walks the tree-sitter tree and emits formatted source.

pub struct Printer {
    output: String,
    indent_level: usize,
    use_tabs: bool,
    indent_size: usize,
}

impl Printer {
    pub fn new(use_tabs: bool, indent_size: usize) -> Self {
        Self {
            output: String::new(),
            indent_level: 0,
            use_tabs,
            indent_size,
        }
    }

    pub fn finish(self) -> String {
        self.output
    }

    pub fn indent(&mut self) {
        self.indent_level += 1;
    }

    pub fn dedent(&mut self) {
        self.indent_level = self.indent_level.saturating_sub(1);
    }

    pub fn write(&mut self, s: &str) {
        self.output.push_str(s);
    }

    pub fn newline(&mut self) {
        self.output.push('\n');
        self.write_indent();
    }

    pub fn blank_line(&mut self) {
        self.output.push_str("\n\n");
        self.write_indent();
    }

    fn write_indent(&mut self) {
        if self.use_tabs {
            for _ in 0..self.indent_level {
                self.output.push('\t');
            }
        } else {
            let spaces = self.indent_level * self.indent_size;
            for _ in 0..spaces {
                self.output.push(' ');
            }
        }
    }
}
