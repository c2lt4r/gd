"""BBCode-to-Markdown converter for Godot documentation strings.

Used by generate_class_db.py and generate_builtins.py to convert
Godot's BBCode markup into clean Markdown for LSP hover/completion.
"""
import re


def bbcode_to_markdown(text: str) -> str:
    """Convert Godot BBCode documentation markup to Markdown."""
    if not text:
        return ""

    s = text

    # [codeblocks] — extract [gdscript] block only
    def replace_codeblocks(m):
        inner = m.group(1)
        gd_match = re.search(
            r'\[gdscript\]\s*(.*?)\s*\[/gdscript\]', inner, re.DOTALL
        )
        if gd_match:
            code = gd_match.group(1).strip()
            return f"\n```gdscript\n{code}\n```\n"
        return ""

    s = re.sub(
        r'\[codeblocks\](.*?)\[/codeblocks\]', replace_codeblocks, s, flags=re.DOTALL
    )

    # [codeblock] — fenced code block
    s = re.sub(
        r'\[codeblock\]\s*(.*?)\s*\[/codeblock\]',
        lambda m: f"\n```\n{m.group(1).strip()}\n```\n",
        s,
        flags=re.DOTALL,
    )

    # [code]...[/code] → inline code
    s = re.sub(r'\[code\](.*?)\[/code\]', r'`\1`', s)

    # [b]...[/b] → bold
    s = re.sub(r'\[b\](.*?)\[/b\]', r'**\1**', s)

    # [i]...[/i] → italic
    s = re.sub(r'\[i\](.*?)\[/i\]', r'*\1*', s)

    # [param x] → `x`
    s = re.sub(r'\[param\s+(\w+)\]', r'`\1`', s)

    # [member x] → `x`
    s = re.sub(r'\[member\s+([\w.]+)\]', r'`\1`', s)

    # [method x] → `x()`
    s = re.sub(r'\[method\s+([\w.]+)\]', r'`\1()`', s)

    # [signal x] → `x`
    s = re.sub(r'\[signal\s+([\w.]+)\]', r'`\1`', s)

    # [constant x] → `x`
    s = re.sub(r'\[constant\s+([\w.]+)\]', r'`\1`', s)

    # [enum x] → `x`
    s = re.sub(r'\[enum\s+([\w.]+)\]', r'`\1`', s)

    # [annotation x] → `x`
    s = re.sub(r'\[annotation\s+([\w.@]+)\]', r'`\1`', s)

    # [theme_item x] → `x`
    s = re.sub(r'\[theme_item\s+([\w.]+)\]', r'`\1`', s)

    # [url=X]text[/url] → [text](X)
    s = re.sub(r'\[url=(.*?)\](.*?)\[/url\]', r'[\2](\1)', s)

    # [url]X[/url] → X
    s = re.sub(r'\[url\](.*?)\[/url\]', r'\1', s)

    # [ClassName] bare references → `ClassName`
    # Match [Word] or [Word.Word] but not already-processed tags
    s = re.sub(r'\[([A-Z]\w*(?:\.\w+)?)\]', r'`\1`', s)

    # [br] → newline
    s = re.sub(r'\[br\]', '\n', s)

    # [lb] / [rb] → literal brackets
    s = s.replace('[lb]', '[').replace('[rb]', ']')

    # Clean up any remaining BBCode-like tags we don't handle
    # (center, img, table, etc.)
    s = re.sub(r'\[/?(?:center|img|table|cell|indent|ol|ul|font)[^\]]*\]', '', s)

    # Collapse multiple blank lines
    s = re.sub(r'\n{3,}', '\n\n', s)

    return s.strip()


def truncate_doc(text: str, max_chars: int = 300) -> str:
    """Truncate a doc string to max_chars, preferring sentence boundaries."""
    if not text or len(text) <= max_chars:
        return text

    # Try to cut at a sentence boundary
    truncated = text[:max_chars]
    # Look for the last sentence-ending punctuation
    last_period = max(truncated.rfind('. '), truncated.rfind('.\n'))
    if last_period > max_chars // 3:
        return truncated[: last_period + 1]

    # Fall back to word boundary
    last_space = truncated.rfind(' ')
    if last_space > max_chars // 2:
        return truncated[:last_space] + "..."

    return truncated + "..."
