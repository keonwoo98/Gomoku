NAME = Gomoku

# Use rustup's cargo/rustc to avoid homebrew version conflicts
CARGO = $(HOME)/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo
RUSTC = $(HOME)/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc

all: $(NAME)

$(NAME): FORCE
	@RUSTC=$(RUSTC) $(CARGO) build --release
	@cp target/release/gomoku $(NAME)

FORCE:

clean:
	@RUSTC=$(RUSTC) $(CARGO) clean

fclean: clean
	@rm -f $(NAME)

re: fclean all

test:
	@RUSTC=$(RUSTC) $(CARGO) test --lib

test-release:
	@RUSTC=$(RUSTC) $(CARGO) test --lib --release

.PHONY: all clean fclean re test test-release
