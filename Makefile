NAME = Gomoku

all: $(NAME)

$(NAME):
	@cargo build --release
	@cp target/release/gomoku $(NAME)

clean:
	@cargo clean

fclean: clean
	@rm -f $(NAME)

re: fclean all

test:
	@cargo test --lib

test-release:
	@cargo test --lib --release

.PHONY: all clean fclean re test test-release
