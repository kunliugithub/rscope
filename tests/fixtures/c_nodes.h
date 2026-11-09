#ifndef RSCOPE_NODES_H
#define RSCOPE_NODES_H
typedef struct Node Node;
struct Node { int value; int (*visit)(int); };
union Data { int integer; double real; };
enum Status { READY, DONE = 1 };
int declared(int value);
extern int external;
#endif
