# Source code를 통한 Rust 언어 특징

## 1. Ownership과 Borrowing

Rust는 메모리 안전성을 보장하기 위해 소유권(ownership) 시스템을 사용합니다.
여기서 `Rc<T>`, `Arc<T>`(Atomic Reference Counting)는 여러 개의 소유자가 안전하게 공유할 수 있도록 참조 카운팅을 지원하는 스마트 포인터입니다.

```rust
router: Rc<Router<String>> // Rc로 single thread용.
router: Arc<Router<String>> // Arc로 공유 (readonly. writable은 Arc<Mutex<T>>, Arc<RwLock<T>>, 비동기에서는 Arc<AsyncMutex<T>>)
```

- `Arc<T>`는 다중 스레드 용이고, `Rc<T>`는 단일 스레드용입니다.
- `Arc::clone(&self.router)`을 사용하여 참조를 복사할 수 있지만, 이는 새로운 복사본이 아니라 같은 데이터를 참조하는 카운트 증가입니다.

## 2. Pattern Matching

Rust는 `match` 문을 사용하여 다양한 경우를 간결하게 처리합니다.

```rust
match self.router.at(&path) {
    Ok(matched) => {
        let openapi_path = matched.value;
        self.set_http_request_header("x-openapi-path", Some(&openapi_path));
    }
    Err(_) => {
        // Handle unmatched routes (optional)
    }
}
```

- `match`를 사용하면 `Ok`와 `Err` 같은 `Result` 타입을 안전하게 처리할 수 있습니다.
- Rust의 오류 처리는 try-catch가 아니라 `Result<T, E> 또는 Option<T>`를 통해 이루어집니다.

## 3. 안전한 문자열 처리

Rust에서는 문자열을 다룰 때 명시적인 변환이 필요합니다.

```rust
let config_str = String::from_utf8(config_bytes.to_vec()).unwrap_or_default();
```

- `String::from_utf8(config_bytes.to_vec())` → 바이트 배열을 UTF-8 문자열로 변환 (실패 시 기본값 반환)

- `unwrap_or_default()` → Err가 발생하면 기본값("")을 반환

Rust에서는 명시적으로 에러 처리를 강제하기 때문에, `unwrap_or_default()` 같은 안전 장치가 자주 사용됩니다.

## 4. Trait과 구현 (Interface 개념)

Rust는 Trait을 사용하여 인터페이스를 정의합니다.

```rust
trait Context {}
trait HttpContext: Context {
    fn on_http_request_headers(&mut self, _: usize, _: bool) -> Action;
}
```

```rust
impl Context for OpenapiPathHttpContext {}
impl HttpContext for OpenapiPathHttpContext {
    fn on_http_request_headers(&mut self, _: usize, _: bool) -> Action {
        // HTTP 요청 헤더 처리 로직
    }
}
```

- trait은 C++의 추상 클래스나 Java의 인터페이스와 유사합니다.
- impl 블록을 사용하여 특정 타입에 대해 trait을 구현합니다.

## 5. 동적 디스패치 (Trait Object)

Rust에서는 `Box<dyn Trait>`을 사용하여 Trait Object(동적 디스패치)를 지원합니다.

> **dispatch**란? 호출 대상 method 결정 과정
>
> - **dynamic dispatch**: runtime 타입 결정. `dyn` 사용 (C++의 경우 `virtual`)
> - **static dispatch**: compile time 타입 결정. `dyn` 없음 (C++의 경우 template)

```rust
fn create_http_context(&self, _: u32) -> Option<Box<dyn HttpContext>> {
    Some(Box::new(OpenapiPathHttpContext {
        router: Arc::clone(&self.router),
    }))
}
```

- `Box<dyn HttpContext>` → `HttpContext`를 구현한 어떤 타입이든 담을 수 있음 (런타임 동적 디스패치)
- C++의 `std::unique_ptr<Base>`나 Java의 업캐스팅과 비슷한 개념

## 6. 기본적인 비동기 실행 모델

Rust 자체는 멀티스레드와 비동기 실행을 지원하지만, 이 코드에서는 직접적인 async/await을 사용하지 않았습니다.
그러나 `Arc<T>`와 같은 구조는 멀티스레드 환경에서 자원을 안전하게 공유하는 데 중요한 역할을 합니다.

## 7. Result와 Error Handling
Rust의 오류 처리는 `Result<T, E>와 Option<T>`를 중심으로 이루어지며, 이 코드에서 configure_router 메서드에서 Result를 적극 활용합니다.

```rust
fn configure_router(&mut self, config: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config: Value = serde_json::from_str(config)?;
    // ...
}
```

`?` 연산자는 내부적 match의 간소화로, Result나 Option에서 오류가 발생하면 함수를 즉시 종료하고 오류를 반환합니다. C++의 예외 처리(throw)와 달리, Rust는 오류를 값으로 처리하여 명시적으로 다루도록 강제합니다.

Box<dyn std::error::Error>가 동적 오류 타입을 다루기 위한 방법입니다.

## 8. `Option<T>`: `Some()`, `None`

`Some()`은 Rust에서 `Option<T>` 타입의 일부로, 값이 존재할 때 사용되는 열거형(Enumeration, Enum) 변형(Variant)입니다.

### 1. `Option<T>` 타입이란?

Rust에서는 `null`이 없기 때문에 값이 없을 수도 있는 경우 `Option<T>` 열거형을 사용합니다.

```rust
enum Option<T> {
    Some(T),
    None,
}
```

- `Some(T)`: 값이 존재할 때 사용
- `None`: 값이 없을 때 사용

이렇게 하면 null 참조 에러 없이 안전하게 값을 다룰 수 있습니다.

### 2. 예제 코드

#### ✅ `Some()`을 사용하는 기본 예제

```rust
fn maybe_number(input: bool) -> Option<i32> {
    if input {
        Some(42)  // 값이 있을 때
    } else {
        None  // 값이 없을 때
    }
}

fn main() {
    let result = maybe_number(true);

    match result {
        Some(num) => println!("Got a number: {}", num),
        None => println!("No number available"),
    }
}
```

출력:

```
Got a number: 42
```

- `maybe_number(true)`는 `Some(42)`를 반환
- `maybe_number(false)`는 `None`을 반환

#### ✅ `Some()`을 활용한 안전한 값 접근 (`unwrap_or`)

`Option<T>` 값을 직접 사용하려면 match 문이나 메서드를 사용해야 합니다.

```rust
let number = Some(10);
let value = number.unwrap_or(0); // Some이면 값 가져오고, None이면 기본값 사용
println!("{}", value); // 출력: 10
```

### 3. 코드에서 `Some()`의 역할

```rust
fn create_http_context(&self, _: u32) -> Option<Box<dyn HttpContext>> {
    Some(Box::new(OpenapiPathHttpContext {
        router: Arc::clone(&self.router),
    }))
}
```

여기서 `Option<Box<dyn HttpContext>>`를 반환하는데,

- `Some(Box::new(...))` → HTTP 컨텍스트 객체가 존재함
- `None` → HTTP 컨텍스트가 없을 때

즉, `create_http_context`가 새로운 HTTP 컨텍스트를 만들 때, 성공하면 `Some(...)`으로 감싸고, 실패하면 `None`을 반환할 수도 있음을 나타냅니다.

### 4. 정리

- `Some(value)`: 값이 있을 때 사용
- `None`: 값이 없을 때 사용
- `Option<T>`를 사용하면 `null` 없이 안전한 코드 작성 가능
- `unwrap_or(default)`, `match` 등을 사용해 값 처리

Rust는 `null` 참조가 없기 때문에, `Option<T>`을 적극적으로 활용해야 합니다.

## 9. `Box::new()`

`Box::new()`는 Rust의 스마트 포인터(Smart Pointer) 중 하나인 `Box<T>`를 생성하는 함수입니다.
**Heap 메모리에 데이터를 저장하고, 그 포인터를 반환하는 역할**을 합니다.

### 1. `Box<T>`가 필요한 이유

Rust는 기본적으로 모든 데이터가 Stack(스택) 메모리에 저장됩니다.
하지만 아래 경우에는 Heap(힙) 메모리에 데이터를 저장해야 합니다.

- 크기가 컴파일 타임에 결정되지 않은 경우 → 컴파일 시 크기를 알 수 없는 데이터 타입(예: 재귀 구조체)을 다룰 때 필요
- 대형 데이터의 소유권을 이동하면서도 복사를 피하고 싶을 때 → 스택이 아닌 힙에 데이터를 저장하여 소유권만 이동할 수 있음
- 동적 디스패치(Trait Object) 사용 시 → Box<dyn Trait>을 사용하면 컴파일 타임이 아닌 런타임에 타입을 결정할 수 있음

### 2. 기본 사용 예제

```rust
let boxed_number = Box::new(42);
println!("Boxed number: {}", boxed_number);
```

#### 실행 과정:

1. 42를 Heap에 저장
2. `Box<T>`는 Heap에 있는 42를 가리키는 포인터 역할
3. boxed_number를 통해 42를 읽거나 조작 가능

### 3. `Box::new()`를 사용하는 이유

Rust는 기본적으로 Trait(트레이트)을 직접 변수로 저장할 수 없습니다.
Trait을 저장하려면 Heap 할당을 통한 동적 디스패치가 필요하며, 이를 위해 `Box<dyn Trait>`을 사용합니다.

```rust
trait Animal {
    fn make_sound(&self);
}

struct Dog;
impl Animal for Dog {
    fn make_sound(&self) {
        println!("Bark!");
    }
}

fn main() {
    let dog: Box<dyn Animal> = Box::new(Dog);
    dog.make_sound(); // "Bark!" 출력
}
```

#### 여기서 `Box::new(Dog)`가 하는 일:

1. Dog 객체를 Heap 메모리에 저장
2. 그 포인터를 `Box<dyn Animal>`에 저장
3. `Box<dyn Animal>`을 통해 런타임 시 `make_sound()` 실행 가능

### 4. Rust 코드에서 `Box::new()`의 역할

Rust 코드를 다시 보면:

```rust
fn create_http_context(&self, _: u32) -> Option<Box<dyn HttpContext>> {
    Some(Box::new(OpenapiPathHttpContext {
        router: Arc::clone(&self.router),
    }))
}
```

#### `Box::new(OpenapiPathHttpContext { ... })`의 역할:

1. `OpenapiPathHttpContext` 객체를 Heap에 저장
2. 포인터(`Box<T>`)를 반환하여 소유권을 유지
3. `Box<dyn HttpContext>`를 반환하여 Trait Object 형태로 저장 가능

즉, `Box<T>`를 사용함으로써 Rust의 엄격한 메모리 모델을 준수하면서도 동적 할당과 다형성을 활용할 수 있는 것입니다.

### 5. 정리

- ✔ `Box::new(T)` → `T`를 Heap에 저장하고 `Box<T>` 반환
- ✔ `Box<T>`는 Heap에 저장된 데이터를 가리키는 스마트 포인터
- ✔ `Box<dyn Trait>`을 사용하면 Trait Object(동적 디스패치)가 가능
- ✔ `Box<T>`는 Rust의 엄격한 메모리 모델을 준수하면서도 동적 할당을 지원하는 방법

Rust의 메모리 안전성과 Trait Object(다형성)를 활용할 때 자주 사용됩니다.

## 10. `#[derive(Default)]`

`#[derive(Default)]`는 Rust에서 구조체(`struct`)나 열거형(`enum`)에 `Default` 트레이트를 자동으로 구현해주는 속성(attribute)입니다

> attribute는 AOP 관점에서 aspect에 해당하나, 다른 점은 aspect가 runtime에 동작하는 것 과는 달리 attribute는 compile time에 동작하는 일종의 매크로.

`Default` 트레이트: 타입의 기본값을 정의하는 트레이트로, `default()` 메서드를 통해 호출됩니다.

`#[derive(Default)]`: 이 속성을 붙이면, Rust 컴파일러가 해당 타입에 대해 `Default` 구현을 자동 생성합니다.

조건: 구조체의 모든 필드가 `Default`를 구현해야 사용 가능합니다 (예: `i32`, `String`, `Vec<T>` 등은 기본적으로 `Default` 제공).

```rust
#[derive(Default)]
struct Example {
    x: i32,       // 기본값: 0
    y: String,    // 기본값: ""
    z: Vec<u8>,   // 기본값: 빈 벡터 []
}

fn main() {
    let ex = Example::default();
    println!("x: {}, y: '{}', z: {:?}", ex.x, ex.y, ex.z);
    // 출력: x: 0, y: '', z: []
}
```

코드에서의 사용:

```rust
#[derive(Default)]
struct OpenapiPathRootContext {
    router: Arc<Router<String>>,
}
```

`Arc<Router<String>>`의 기본값은 `Default`를 통해 생성되며, `Router<String>`이 `Default`를 지원해야 합니다.

`OpenapiPathRootContext::default()`를 호출하면 `router`가 초기화된 기본 인스턴스를 얻을 수 있습니다.