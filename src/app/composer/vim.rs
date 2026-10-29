use super::Composer;

impl Composer<'_> {
    pub(crate) fn vim_word_key(&mut self, key: char) {
        if self.pending_vim_g {
            self.pending_vim_g = false;
            if key == 'e' || key == 'E' {
                self.move_word(key == 'E', WordMotion::PreviousEnd);
            }
            return;
        }
        match key {
            'g' => self.pending_vim_g = true,
            'w' => self.move_word(false, WordMotion::NextStart),
            'W' => self.move_word(true, WordMotion::NextStart),
            'b' => self.move_word(false, WordMotion::PreviousStart),
            'B' => self.move_word(true, WordMotion::PreviousStart),
            'e' => self.move_word(false, WordMotion::NextEnd),
            'E' => self.move_word(true, WordMotion::NextEnd),
            _ => {}
        }
    }

    pub(crate) fn vim_yank_key(&mut self, key: char) -> Option<String> {
        if !self.pending_vim_y {
            if key == 'y' {
                self.pending_vim_y = true;
            }
            return None;
        }
        if self.pending_vim_g {
            self.pending_vim_g = false;
            self.pending_vim_y = false;
            return match key {
                'e' => self.word_yank(false, WordMotion::PreviousEnd),
                'E' => self.word_yank(true, WordMotion::PreviousEnd),
                _ => None,
            };
        }
        match key {
            'g' => {
                self.pending_vim_g = true;
                None
            }
            'y' => {
                self.pending_vim_y = false;
                let row = self.input.cursor().0;
                let lines = self.input.lines();
                lines.get(row).map(|line| format!("{line}\n"))
            }
            _ => {
                self.pending_vim_y = false;
                match key {
                    'w' => self.word_yank(false, WordMotion::NextStart),
                    'W' => self.word_yank(true, WordMotion::NextStart),
                    'b' => self.word_yank(false, WordMotion::PreviousStart),
                    'B' => self.word_yank(true, WordMotion::PreviousStart),
                    'e' => self.word_yank(false, WordMotion::NextEnd),
                    'E' => self.word_yank(true, WordMotion::NextEnd),
                    _ => None,
                }
            }
        }
    }

    pub(crate) fn vim_yank_pending(&self) -> bool {
        self.pending_vim_y
    }

    fn word_yank(&self, big: bool, motion: WordMotion) -> Option<String> {
        let text = self.text();
        let chars: Vec<char> = text.chars().collect();
        if chars.is_empty() {
            return None;
        }
        let offset = self.cursor_offset().min(chars.len());
        let target = word_target(&chars, offset, big, motion);
        let range = match motion {
            WordMotion::NextStart | WordMotion::PreviousStart => {
                offset.min(target)..offset.max(target)
            }
            WordMotion::NextEnd | WordMotion::PreviousEnd => {
                if target < offset {
                    target..offset
                } else {
                    offset..target.saturating_add(1).min(chars.len())
                }
            }
        };
        (!range.is_empty()).then(|| chars[range].iter().collect())
    }

    pub(crate) fn vim_word_prefix_pending(&self) -> bool {
        self.pending_vim_g
    }

    pub(crate) fn clear_vim_word_prefix(&mut self) {
        self.pending_vim_g = false;
        self.pending_vim_y = false;
    }

    fn move_word(&mut self, big: bool, motion: WordMotion) {
        let chars: Vec<char> = self.text().chars().collect();
        if chars.is_empty() {
            return;
        }
        let offset = self.cursor_offset().min(chars.len());
        let target = word_target(&chars, offset, big, motion);
        self.set_cursor_offset(target);
        self.caret_affinity = None;
        self.preferred_visual_column = None;
    }
}

#[derive(Clone, Copy)]
enum WordMotion {
    NextStart,
    PreviousStart,
    NextEnd,
    PreviousEnd,
}

fn word_target(chars: &[char], offset: usize, big: bool, motion: WordMotion) -> usize {
    let class = |index: usize| word_class(chars[index], big);
    let target = match motion {
        WordMotion::NextStart => {
            let mut next = offset;
            if next < chars.len() && class(next) != 0 {
                let current = class(next);
                while next < chars.len() && class(next) == current {
                    next += 1;
                }
            }
            while next < chars.len() && class(next) == 0 {
                next += 1;
            }
            next
        }
        WordMotion::PreviousStart => {
            let mut previous = offset.saturating_sub(1);
            while previous > 0 && class(previous) == 0 {
                previous -= 1;
            }
            if class(previous) != 0 {
                let current = class(previous);
                while previous > 0 && class(previous - 1) == current {
                    previous -= 1;
                }
            }
            previous
        }
        WordMotion::NextEnd => {
            let mut next = offset.saturating_add(1).min(chars.len());
            while next < chars.len() && class(next) == 0 {
                next += 1;
            }
            if next < chars.len() {
                let current = class(next);
                while next + 1 < chars.len() && class(next + 1) == current {
                    next += 1;
                }
                next
            } else {
                chars.len().saturating_sub(1)
            }
        }
        WordMotion::PreviousEnd => {
            let mut previous = offset.saturating_sub(1);
            while previous > 0 && class(previous) == 0 {
                previous -= 1;
            }
            if offset < chars.len()
                && previous + 1 == offset
                && class(offset) != 0
                && class(previous) == class(offset)
            {
                while previous > 0 && class(previous - 1) == class(previous) {
                    previous -= 1;
                }
                previous = previous.saturating_sub(1);
                while previous > 0 && class(previous) == 0 {
                    previous -= 1;
                }
            }
            previous
        }
    };
    target
}

fn word_class(character: char, big: bool) -> u8 {
    if character.is_whitespace() {
        0
    } else if big || character.is_alphanumeric() || character == '_' {
        1
    } else {
        2
    }
}
