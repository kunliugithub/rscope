struct report { int size; char data[] __counted_by(size); };
