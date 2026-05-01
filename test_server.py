import requests
import json

# Test the aether-server API
def test_analyze_endpoint():
    url = "http://localhost:3000/analyze"
    
    # Test case 1: High trust code
    request_data = {
        "code": """@prov(source: "user", confidence: 1.0)
fn verified_add(a: Int, b: Int) -> Int {
    a + b
}

@prov(source: "claude", confidence: 0.9)
fn ai_helper(n: Int) -> Int {
    n * 2
}

fn main() -> Int {
    verified_add(ai_helper(5), 1)
}""",
        "min_trust": 0.8
    }
    
    print("Testing high trust code...")
    try:
        response = requests.post(url, json=request_data, timeout=5)
        print(f"Status: {response.status_code}")
        print(f"Response: {json.dumps(response.json(), indent=2)}")
    except requests.exceptions.RequestException as e:
        print(f"Error: {e}")
    
    print("\n" + "="*50 + "\n")
    
    # Test case 2: Low trust code
    request_data = {
        "code": """@prov(source: "claude", confidence: 0.3)
fn low_trust_func(a: Int, b: Int) -> Int {
    a + b
}

fn main() -> Int {
    low_trust_func(5, 10)
}""",
        "min_trust": 0.8
    }
    
    print("Testing low trust code...")
    try:
        response = requests.post(url, json=request_data, timeout=5)
        print(f"Status: {response.status_code}")
        print(f"Response: {json.dumps(response.json(), indent=2)}")
    except requests.exceptions.RequestException as e:
        print(f"Error: {e}")

if __name__ == "__main__":
    test_analyze_endpoint()
