export interface MinecraftPart {
    text: string;
    color?: string;
    bold?: boolean;
    italic?: boolean;
    underlined?: boolean;
    strikethrough?: boolean;
    obfuscated?: boolean;
}

const COLOR_CODES: Record<string, string> = {
    '0': '#000000', // black
    '1': '#0000AA', // dark_blue
    '2': '#00AA00', // dark_green
    '3': '#00AAAA', // dark_aqua
    '4': '#AA0000', // dark_red
    '5': '#AA00AA', // dark_purple
    '6': '#FFAA00', // gold
    '7': '#AAAAAA', // gray
    '8': '#555555', // dark_gray
    '9': '#5555FF', // blue
    'a': '#55FF55', // green
    'b': '#55FFFF', // aqua
    'c': '#FF5555', // red
    'd': '#FF55FF', // light_purple
    'e': '#FFFF55', // yellow
    'f': '#FFFFFF', // white
};

export function parseMinecraftCodes(text: string): MinecraftPart[] {
    const parts: MinecraftPart[] = [];
    let currentPart: MinecraftPart = { 
        text: '',
        color: undefined,
        bold: false,
        italic: false,
        underlined: false,
        strikethrough: false,
        obfuscated: false
    };
    
    for (let i = 0; i < text.length; i++) {
        if (text[i] === '§' && i + 1 < text.length) {
            const code = text[i + 1].toLowerCase();
            i++; // skip code
            
            if (COLOR_CODES[code]) {
                if (currentPart.text) parts.push({ ...currentPart });
                currentPart.text = '';
                currentPart.color = COLOR_CODES[code];
                // Reset styles when color changes? Usually Minecraft does this for colors, but not styles.
                // Actually, in Minecraft, a color code resets all previous styles.
                currentPart.bold = false;
                currentPart.italic = false;
                currentPart.underlined = false;
                currentPart.strikethrough = false;
                currentPart.obfuscated = false;
            } else if (code === 'l') {
                if (currentPart.text) parts.push({ ...currentPart });
                currentPart.text = '';
                currentPart.bold = true;
            } else if (code === 'm') {
                if (currentPart.text) parts.push({ ...currentPart });
                currentPart.text = '';
                currentPart.strikethrough = true;
            } else if (code === 'n') {
                if (currentPart.text) parts.push({ ...currentPart });
                currentPart.text = '';
                currentPart.underlined = true;
            } else if (code === 'o') {
                if (currentPart.text) parts.push({ ...currentPart });
                currentPart.text = '';
                currentPart.italic = true;
            } else if (code === 'k') {
                if (currentPart.text) parts.push({ ...currentPart });
                currentPart.text = '';
                currentPart.obfuscated = true;
            } else if (code === 'r') {
                if (currentPart.text) parts.push({ ...currentPart });
                currentPart.text = '';
                currentPart.color = undefined;
                currentPart.bold = false;
                currentPart.italic = false;
                currentPart.underlined = false;
                currentPart.strikethrough = false;
                currentPart.obfuscated = false;
            }
        } else {
            currentPart.text += text[i];
        }
    }
    
    if (currentPart.text) parts.push(currentPart);
    return parts;
}
