use std::{fmt::{self, Debug as D}, io::*, collections::HashMap};
mod sibling;
macro_rules! wrap { ($e:expr) => { hidden($e) }; }
struct Counter { value: i32 }
trait Visit { fn visit(&self); }
const LIMIT: i32 = 2;
fn helper(value: i32) -> i32 { value }
impl Counter {
    fn run(&mut self) {
        let (mut left, right) = (helper(1), 2);
        self.value = left;
        left += right;
        let mut values = [0; 2];
        values[0] = helper(left);
        let callback = || helper(3);
        wrap!(hidden(4));
        helper::<i32>(5);
        self.visit();
        let Counter { value: renamed } = Counter { value: 1 };
    }
}
// phantom needle
const MESSAGE: &str = "phantom needle";
