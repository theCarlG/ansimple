
# Ansimple

Simplified ansible clone written in Rust 🦀. 

This project made for fun and not intended for production use.


## Host config example
```yaml
global_config:
  user: "someuser"
  key: "/home/someuser/.ssh/id_ed25519"

hosts:
  - address: host1
  - address: host2
```

## Playbook example

```yaml
hosts:
  - host1
  - host2

tasks:
- shell:
    name: check system uptime
    command: uptime
  tags:
    - diagnostics

- shell:
    name: update apt repo
    command: sudo apt update
  tags:
    - maintenance

- copy:
    name: copy local file to remote
    src: ./file.txt
    dest: /tmp/file.txt
  tags:
    - text_processing
    - files
    - remote_copy

- search_replace:
    name: search and replace in remote file
    path: /tmp/file.txt
    search: "\\d+"  # Regular expression to match numbers
    replace: "0"
  tags:
    - text_processing
    - remote_copy
  register: search_and_replace

- copy:
    name: copy remote file to location
    src: /tmp/file.txt
    dest: /tmp/file2.txt
    remote_src: true
  tags:
    - remote_copy

- shell:
    name: cat file2.txt
    command: cat /tmp/file2.txt
  tags:
    - remote_copy

- template:
    name: copy and render local template
    src: ./template.j2
    dest: /tmp/template.txt
    variables:
      var1: "value1"
      var2: "value2"
  tags:
    - templates
  register: templating

- shell:
    name: cat template.txt
    command: cat /tmp/template.txt
  tags:
    - templates
  when: templating == "changed"
```


