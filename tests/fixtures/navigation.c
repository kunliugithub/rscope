#include "c_nodes.h"
#include <stddef.h>
#define SCALE(x) ((x) * 2)
#define LIMIT 10
static int counter = 0;
int leaf(int value) { return value; }
int *pointer_result(void) { return &counter; }
int (*factory(void))(int) { return leaf; }
int (*callback)(int) = leaf;
int no_initializer;

int outer(Node *node) {
    int local = leaf(1);
    node->value = local;
    counter += 1;
    local++;
    int values[2] = {0, 1};
    values[0] = leaf(local);
    node->visit(local);
    (*callback)(local);
    int *ptr = &local;
    *ptr = 2;
    const char *message = "phantom leaf outer"; /* phantom() is not a call */
    (void)message;
    return SCALE(local);
}
#if FEATURE
int selected(void) { return leaf(LIMIT); }
#else
int selected(void) { return leaf(0); }
#endif
