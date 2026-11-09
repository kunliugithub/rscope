import os
import package.module as alias, sys
from package.tools import run as execute
from .relative import Item
from .. import thing

# text-only needle [x].* 100%_ literal -needle café
MESSAGE = "needle needle"
counter: int = 0
annotation_only: int
first = second = 1
left, (right, *rest) = (1, (2, 3, 4))
obj.field = counter
mapping[key] = 1
counter += 1
if (found := lookup()):
    counter = found

def work():
    # comment-only needle
    import local.module as local_alias
    from local.tools import execute
    local_value = "NEEDLE"
    return local_value

text = """multiline needle
    source-only needle line without identifiers
"""
