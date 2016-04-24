//! Text wrapping module


/// Defines wrapping points for a string wrapped to a particular width.
#[derive(Debug, Clone)]
pub struct StringWrap {
    /// Character indices at which the text was wrapped.
    points: Vec<usize>,
}

impl StringWrap {
    /// Wraps `text` to the given width.
    pub fn new(text: &str, width: usize) -> StringWrap {
        let mut points = vec![0];

        // Track the last index where we saw a space.
        let mut last_spc = None;
        let mut last_split = 0;
        for (i, ch) in text.char_indices() {
            let x = i - last_split;
            // If we've exceeded our width, add a wrap point at the last space.
            if x > width {
                if let Some(p) = last_spc {
                    points.push(p + 1);
                    last_split = p + 1;
                } else {
                    // If there's no space to wrap to, we have to break the line
                    // at our current position.
                    points.push(i);
                    last_split = i;
                }
            }

            if ch == ' ' { last_spc = Some(i); }
        }

        StringWrap {
            points: points,
        }
    }


    /// Gets the x and y offset of the given index in the string.
    pub fn idx_pos(&self, idx: usize) -> (isize, isize) {
        // This will store the wrap point that occurs right before the index.
        let mut point = 0;
        // This will store the line that point is on.
        let mut line = 0;
        for (i, &p) in self.points.iter().enumerate() {
            if p < idx {
                point = p;
                line = i;
            } else {
                break;
            }
        }
        let x = idx - point;
        let y = line;
        (x as isize, y as isize)
    }


    /// Returns the number of lines the string was wrapped to.
    pub fn line_count(&self) -> usize {
        self.points.len()
    }

    /// Splits the given string at this wrapping's wrap points and returns an
    /// iterator over the resulting sub-string slices.
    pub fn iter_lines<'a>(&'a self, text: &'a str) -> IterLines<'a> {
        IterLines {
            text: text,
            wrap: self,
            point: 0,
        }
    }
}

#[derive(Debug)]
pub struct IterLines<'a> {
    text: &'a str,
    wrap: &'a StringWrap,
    point: usize,
}

impl<'a> Iterator for IterLines<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.point <= self.wrap.points.len() {
            let start = if self.point > 0 {
                self.wrap.points[self.point-1]
            } else { 0 };
            let end = if self.point < self.wrap.points.len() {
                self.wrap.points[self.point]
            } else { self.text.len() };

            self.point += 1;
            Some(&self.text[start..end])
        } else {
            None
        }
    }
}
