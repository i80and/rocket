## Introduction [![Build Status](https://travis-ci.org/i80and/rocket.svg?branch=master)](https://travis-ci.org/i80and/rocket)

```
(:let (:name Rocket) =>
  (:name) is a fast, powerful, and homoiconic text markup format with tight integration with
  [Markdown](https://daringfireball.net/projects/markdown/).

  (:note =>
    (:name) is a (:concat bit " " of " " a) mishmash, I'll admit. But hopefully
    a useful one!

```

## Directives

### Null

A no-op directive..

```
(:null)
```

### Let

Sets a set of variables for use in child expressions.

```
(:let (:<expr-var1> <expr-value1>
        <expr-var2> <expr-value2>...)
      <expr>)
```

### Version

```
(:version [<expr>])
```

### Concat

```
(:concat <expr> [<expr>, [...]])
```

### Markdown

```
(:md <expr>)
```

### Definition List

```
(:definition-list
    (:<expr-term1> <expr-definition1>)
    [(:<expr-term1> <expr-definition1>) [...]]
)
```

### Theme Configuration

```
(:theme-config
  title "The Rocket Compiler"
  date "2017-06-13"
)
```

### Define

```
(:define foo bar)
(:foo)
```

### Simple Templates

```
(:define-template <name> <pattern> [<regex>, [<regex>, ...]])
```

### Include

```
(:include <expr-path>)
```

### Include

Imports the given file's definitions, but returns an empty string.

```
(:import <expr-path>)
```

### Admonitions

#### Note

```
(:note [<expr-title>] <expr-body>)
```

#### Warning

```
(:warning [<expr-title>] <expr-body>)
```
