import java.util.List;
import static java.util.Collections.*;
interface Visit { void visit(); }
record Pair(int first, int second) {}
class Navigation implements Visit {
    int value = 1;
    Navigation() { this(1); }
    Navigation(int input) { value = input; }
    static int helper(int input) { return input; }
    public void visit() {}
    void run() {
        int left = helper(1), right;
        this.value = left;
        left++;
        int[] values = new int[2];
        values[0] += helper(left);
        Runnable callback = () -> helper(3);
        Navigation created = new Navigation();
        created.visit();
        Object anon = new Object() { void nested() { helper(4); } };
    }
    // phantom needle
    String message = "phantom needle";
}
