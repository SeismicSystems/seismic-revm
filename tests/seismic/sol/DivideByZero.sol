contract DivideByZero {
    function test() public {
        suint256 a = 10;
        suint256 b = 2;
        DivZeroTester tester = new DivZeroTester();

        // Test valid division
        tester.divide(a, b);

        // Test division by zero
        try tester.divide(a, 0) {
            return;
        } catch Panic(uint256 errorCode) {
            // Ensure the revert reason is correct
            assert(errorCode == 0x12);
        }
    }
}

contract DivZeroTester {
    function divide(suint256 a, suint256 b) external pure {
        a / b;
    }
}