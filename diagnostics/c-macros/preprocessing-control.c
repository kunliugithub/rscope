/* Deliberately simplified macro, not the kernel's container_of implementation. */
#define container_of(pointer, type, member) ((type *)(pointer))
struct item { int value; };
void f(void *ptr)
{
    struct item *obj = container_of(ptr, struct item, value);
    (void)obj;
}
