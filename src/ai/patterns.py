"""
Pattern definitions for Gomoku heuristic evaluation.
Defines scoring for various stone patterns.
"""

from enum import IntEnum


class PatternScore(IntEnum):
    """Score values for different patterns."""
    FIVE = 1_000_000          # Win
    OPEN_FOUR = 100_000       # Unstoppable (opponent must block)
    FOUR = 10_000             # One move from winning
    OPEN_THREE = 5_000        # Two moves from winning, hard to block
    THREE = 500               # Potential
    OPEN_TWO = 100            # Building potential
    TWO = 10                  # Minimal value

    # Capture-related
    CAPTURE_MADE = 5_000      # Per pair captured
    CAPTURE_THREAT = 3_000    # Can capture opponent
    CAPTURE_DANGER = -2_000   # At risk of being captured

    # Special bonuses
    FOUR_CAPTURES = 50_000    # Close to capture win
    CENTER_CONTROL = 50       # Bonus for center positions


class Pattern:
    """
    Pattern representation for matching.
    Uses string patterns where:
    - 'X' = our stone
    - 'O' = opponent stone
    - '_' = empty
    - '?' = any (don't care)
    - '*' = out of bounds or opponent
    """

    def __init__(self, pattern: str, score: int, name: str = ""):
        self.pattern = pattern
        self.score = score
        self.name = name
        self.length = len(pattern)

    def __repr__(self):
        return f"Pattern({self.name}: {self.pattern} = {self.score})"


# Pattern definitions (from most valuable to least)
# These are checked in order, first match wins
ATTACK_PATTERNS = [
    # Five in a row (win)
    Pattern("XXXXX", PatternScore.FIVE, "FIVE"),

    # Open Four (guaranteed win next turn)
    Pattern("_XXXX_", PatternScore.OPEN_FOUR, "OPEN_FOUR"),

    # Four (one away from win)
    Pattern("XXXX_", PatternScore.FOUR, "FOUR_RIGHT"),
    Pattern("_XXXX", PatternScore.FOUR, "FOUR_LEFT"),
    Pattern("XXX_X", PatternScore.FOUR, "FOUR_GAP1"),
    Pattern("XX_XX", PatternScore.FOUR, "FOUR_GAP2"),
    Pattern("X_XXX", PatternScore.FOUR, "FOUR_GAP3"),

    # Open Three (can become open four)
    Pattern("_XXX_", PatternScore.OPEN_THREE, "OPEN_THREE"),
    Pattern("_XX_X_", PatternScore.OPEN_THREE, "OPEN_THREE_GAP1"),
    Pattern("_X_XX_", PatternScore.OPEN_THREE, "OPEN_THREE_GAP2"),

    # Three (half-open)
    Pattern("XXX__", PatternScore.THREE, "THREE_RIGHT"),
    Pattern("__XXX", PatternScore.THREE, "THREE_LEFT"),
    Pattern("XX_X_", PatternScore.THREE, "THREE_GAP1"),
    Pattern("_X_XX", PatternScore.THREE, "THREE_GAP2"),
    Pattern("X_XX_", PatternScore.THREE, "THREE_GAP3"),
    Pattern("_XX_X", PatternScore.THREE, "THREE_GAP4"),

    # Open Two
    Pattern("_XX_", PatternScore.OPEN_TWO, "OPEN_TWO"),
    Pattern("_X_X_", PatternScore.OPEN_TWO, "OPEN_TWO_GAP"),

    # Two
    Pattern("XX___", PatternScore.TWO, "TWO_RIGHT"),
    Pattern("___XX", PatternScore.TWO, "TWO_LEFT"),
    Pattern("X_X__", PatternScore.TWO, "TWO_GAP1"),
    Pattern("__X_X", PatternScore.TWO, "TWO_GAP2"),
]


def get_pattern_score(line: list, our_stone: int, opp_stone: int) -> int:
    """
    Evaluate a line of stones and return the pattern score.

    Args:
        line: List of stone values (our_stone, opp_stone, or 0 for empty)
        our_stone: Value representing our stones
        opp_stone: Value representing opponent stones

    Returns:
        Score for the most valuable pattern found
    """
    # Convert line to pattern string
    pattern_str = ""
    for cell in line:
        if cell == our_stone:
            pattern_str += "X"
        elif cell == opp_stone:
            pattern_str += "O"
        elif cell == 0:
            pattern_str += "_"
        else:  # Out of bounds or invalid
            pattern_str += "*"

    # Check each pattern
    total_score = 0
    for pattern in ATTACK_PATTERNS:
        # Find all occurrences of pattern
        idx = 0
        while True:
            pos = pattern_str.find(pattern.pattern, idx)
            if pos == -1:
                break
            total_score += pattern.score
            idx = pos + 1

    return total_score


def count_pattern(line_str: str, pattern: str) -> int:
    """Count occurrences of a pattern in a line string."""
    count = 0
    idx = 0
    while True:
        pos = line_str.find(pattern, idx)
        if pos == -1:
            break
        count += 1
        idx = pos + 1
    return count


def line_to_string(line: list, our_stone: int, opp_stone: int) -> str:
    """Convert a line of stones to pattern string."""
    result = ""
    for cell in line:
        if cell == our_stone:
            result += "X"
        elif cell == opp_stone:
            result += "O"
        elif cell == 0:
            result += "_"
        else:
            result += "*"
    return result
