contract UncheckedArithmetic {
    function test() public pure {
        uint256 MAX = type(uint256).max;
        uint256 MIN = type(uint256).min;

        suint256 a = MAX;
        unchecked {
            suint256 b = a + 1;
            assert(b == 0);
        }

        a = MIN;
        unchecked {
            suint256 b = a - 1;
            assert(b == MAX);
        }
    }
}