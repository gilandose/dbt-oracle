import subprocess
import json
import time

def send_message(proc, payload):
    content = json.dumps(payload)
    message = f"Content-Length: {len(content)}\r\n\r\n{content}"
    proc.stdin.write(message.encode('utf-8'))
    proc.stdin.flush()

def read_message(proc):
    header = proc.stdout.readline().decode('utf-8')
    if not header:
        return None
    proc.stdout.readline() # Consume \r\n
    length = int(header.split(':')[1].strip())
    content = proc.stdout.read(length).decode('utf-8')
    return json.loads(content)

print("🤖 [AGENT] Booting up the dbt LSP engine in the background...")
proc = subprocess.Popen(
    ['./target/debug/dbt-oracle'],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE
)

# 1. Agent sends Initialize request
init_payload = {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "initialize",
    "params": {
        "processId": None,
        "rootUri": None,
        "capabilities": {}
    }
}
send_message(proc, init_payload)
resp = read_message(proc)
print(f"✅ [LSP] Initialized successfully. Server version: {resp['result']['serverInfo']['version']}")

init_notif = {
    "jsonrpc": "2.0",
    "method": "initialized",
    "params": {}
}
send_message(proc, init_notif)

# Read the log message from initialized
log_msg1 = read_message(proc)
log_msg2 = read_message(proc) # load manifest log

# 2. Agent opens a file in memory
sql_text = """
SELECT 
    customer_id, 
    email 
FROM {{ ref('stg_customers') }}
"""
did_open = {
    "jsonrpc": "2.0",
    "method": "textDocument/didOpen",
    "params": {
        "textDocument": {
            "uri": "file:///workspace/models/test.sql",
            "languageId": "sql",
            "version": 1,
            "text": sql_text
        }
    }
}
send_message(proc, did_open)

# The server sends diagnostics first (since we opened a file)
diag = read_message(proc)

# 3. Agent asks: "What is the schema for 'customer_id' at line 2, character 5?"
hover_payload = {
    "jsonrpc": "2.0",
    "id": 2,
    "method": "textDocument/hover",
    "params": {
        "textDocument": {"uri": "file:///workspace/models/test.sql"},
        "position": {"line": 2, "character": 5} # Hovering right over "customer_id"
    }
}
print("\n🤖 [AGENT] Asking LSP: 'What is the data type and description of the column I am looking at?'")
send_message(proc, hover_payload)
hover_resp = read_message(proc)

print("✅ [LSP] Reply:")
if hover_resp.get('result'):
    contents = hover_resp['result']['contents']
    if isinstance(contents, str):
        print(contents)
    else:
        print(contents.get('value', contents))
else:
    print("No hover info found.", hover_resp)

# 4. Agent asks: "What is the lineage for 'stg_customers' at line 4, character 15?"
hover_payload_2 = {
    "jsonrpc": "2.0",
    "id": 3,
    "method": "textDocument/hover",
    "params": {
        "textDocument": {"uri": "file:///workspace/models/test.sql"},
        "position": {"line": 4, "character": 15} # Hovering right over "stg_customers"
    }
}
print("\n🤖 [AGENT] Asking LSP: 'What is the upstream and downstream lineage of this model?'")
send_message(proc, hover_payload_2)
hover_resp_2 = read_message(proc)

print("✅ [LSP] Reply:")
if hover_resp_2.get('result'):
    contents = hover_resp_2['result']['contents']
    if isinstance(contents, str):
        print(contents)
    else:
        print(contents.get('value', contents))
else:
    print("No hover info found.", hover_resp_2)

proc.terminate()
